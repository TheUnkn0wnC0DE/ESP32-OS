/* ESP32-OS, B2: Transport der PC-Tests (c/transport.h). Nur http über
 * Winsock zu einem Prüfserver auf dem PC (netsurf/src/lib.rs); jede Anfrage
 * läuft wie am Board in einem eigenen Faden vollständig durch (HTTP/1.0,
 * Verbindung schliesst) – NetSurf läuft derweil weiter (B7: eine langsame
 * Antwort hält die anderen nicht auf). Danach werden die Ereignisse
 * nacheinander abgeholt. https meldet einen Fehler:
 * TLS mit Zertifikatsprüfung gibt es nur am Board (c/transport_esp.c) –
 * ausser mit NS_PROXY=host:port: dann gehen http- und https-Anfragen als
 * „GET <ganze Adresse>“ an diesen Proxy (netsurf/werkzeuge/proxy.py holt
 * die echten Seiten mit Zertifikatsprüfung), um echte Seiten am PC
 * nachzustellen.
 * Wird ohne ns_programm.h übersetzt (Speicher der C-Laufzeit, nur hier
 * angelegt und freigegeben). */
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <winsock2.h>
#include <ws2tcpip.h>

#include "transport.h"

#define MAX_EREIGNISSE 2048

struct ns_transport {
	struct ns_transport_ereignis e[MAX_EREIGNISSE];
	char *texte[MAX_EREIGNISSE];
	int n, naechstes;
	char *antwort;
	size_t antwort_n;
	/* Faden der Anfrage; Ereignisse erst nach seinem Ende abholen */
	HANDLE faden;
	volatile LONG fertig;
	/* Besitzer (Aufrufer, Faden): wer zuletzt loslässt, gibt frei – ein
	 * Abbruch wartet nicht auf einen langsamen Server */
	volatile LONG besitzer;
	char *url, *methode, *kopfzeilen, *agent;
	void *koerper;
	size_t koerper_n;
};

static void dazu(struct ns_transport *t, enum ns_transport_art art, long status, const char *text, size_t tl,
		 const uint8_t *daten, size_t n)
{
	struct ns_transport_ereignis *e;
	if (t->n >= MAX_EREIGNISSE)
		return;
	e = &t->e[t->n];
	memset(e, 0, sizeof *e);
	e->art = art;
	e->status = status;
	if (text != NULL) {
		t->texte[t->n] = malloc(tl + 1);
		if (t->texte[t->n] != NULL) {
			memcpy(t->texte[t->n], text, tl);
			t->texte[t->n][tl] = '\0';
		}
		e->text = t->texte[t->n];
	}
	e->daten = daten;
	e->n = n;
	t->n++;
}

static void fehler(struct ns_transport *t, const char *text)
{
	dazu(t, NS_T_FEHLER, 0, text, strlen(text), NULL, 0);
}

/* http(s)://host[:port]/pfad → Teile; false bei anderem Schema (https nur
 * mit `https_auch`) */
static bool zerlegen(const char *url, char *host, size_t hmax, char *port, size_t pmax, const char **pfad,
		     bool https_auch)
{
	const char *h, *ende, *doppel;
	size_t l;
	bool tls = false;
	if (strncmp(url, "http://", 7) == 0) {
		h = url + 7;
	} else if (https_auch && strncmp(url, "https://", 8) == 0) {
		h = url + 8;
		tls = true;
	} else {
		return false;
	}
	ende = strchr(h, '/');
	if (ende == NULL)
		ende = h + strlen(h);
	doppel = memchr(h, ':', (size_t)(ende - h));
	l = (size_t)((doppel ? doppel : ende) - h);
	if (l >= hmax)
		return false;
	memcpy(host, h, l);
	host[l] = '\0';
	if (doppel) {
		l = (size_t)(ende - doppel - 1);
		if (l >= pmax)
			return false;
		memcpy(port, doppel + 1, l);
		port[l] = '\0';
	} else {
		snprintf(port, pmax, tls ? "443" : "80");
	}
	*pfad = *ende ? ende : "/";
	return true;
}

/* Winsock einmal im Hauptfaden starten (vor dem ersten Faden) */
static void wsa_starten(void)
{
	static bool wsa;
	if (!wsa) {
		WSADATA w;
		WSAStartup(MAKEWORD(2, 2), &w);
		wsa = true;
	}
}

static void anfragen(struct ns_transport *t, const char *url, const char *methode, const char *kopfzeilen,
		     const void *koerper, size_t koerper_n, const char *agent)
{
	char host[256], port[8], kopf[16384], ziel_host[256], ziel_port[8];
	const char *pfad, *ziel;
	const char *proxy = getenv("NS_PROXY");
	struct addrinfo hint, *ai = NULL;
	SOCKET s;
	int l;
	char *p, *ende, *z;
	long status;
	if (!zerlegen(url, host, sizeof host, port, sizeof port, &pfad, proxy != NULL && proxy[0] != '\0')) {
		fehler(t, strncmp(url, "https://", 8) == 0 ? "https gibt es in den PC-Tests nicht (TLS nur am Board)"
							   : "Adresse nicht verstanden");
		return;
	}
	/* Verbindung zum Proxy statt zum Rechner der Adresse; Anfrage mit der
	 * ganzen Adresse */
	ziel = pfad;
	snprintf(ziel_host, sizeof ziel_host, "%s", host);
	snprintf(ziel_port, sizeof ziel_port, "%s", port);
	if (proxy != NULL && proxy[0] != '\0') {
		const char *dp = strrchr(proxy, ':');
		if (dp == NULL || (size_t)(dp - proxy) >= sizeof ziel_host) {
			fehler(t, "NS_PROXY: host:port erwartet");
			return;
		}
		memcpy(ziel_host, proxy, (size_t)(dp - proxy));
		ziel_host[dp - proxy] = '\0';
		snprintf(ziel_port, sizeof ziel_port, "%s", dp + 1);
		ziel = url;
	}
	memset(&hint, 0, sizeof hint);
	hint.ai_family = AF_INET;
	hint.ai_socktype = SOCK_STREAM;
	if (getaddrinfo(ziel_host, ziel_port, &hint, &ai) != 0 || ai == NULL) {
		fehler(t, "Rechner nicht gefunden");
		return;
	}
	s = socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
	if (s == INVALID_SOCKET || connect(s, ai->ai_addr, (int)ai->ai_addrlen) != 0) {
		freeaddrinfo(ai);
		if (s != INVALID_SOCKET)
			closesocket(s);
		fehler(t, "Verbindung abgelehnt");
		return;
	}
	freeaddrinfo(ai);
	l = snprintf(kopf, sizeof kopf, "%s %s HTTP/1.0\r\nHost: %s\r\nUser-Agent: %s\r\nConnection: close\r\n%s", methode,
		     ziel, host, agent ? agent : "NetSurf", kopfzeilen ? kopfzeilen : "");
	if (koerper_n > 0 && l > 0 && (size_t)l < sizeof kopf)
		l += snprintf(kopf + l, sizeof kopf - (size_t)l, "Content-Length: %u\r\n", (unsigned)koerper_n);
	if (l > 0 && (size_t)l < sizeof kopf)
		l += snprintf(kopf + l, sizeof kopf - (size_t)l, "\r\n");
	send(s, kopf, l, 0);
	if (koerper_n > 0)
		send(s, koerper, (int)koerper_n, 0);
	for (;;) {
		char puffer[4096];
		int n = recv(s, puffer, sizeof puffer, 0);
		char *neu;
		if (n <= 0)
			break;
		neu = realloc(t->antwort, t->antwort_n + (size_t)n + 1);
		if (neu == NULL)
			break;
		t->antwort = neu;
		memcpy(t->antwort + t->antwort_n, puffer, (size_t)n);
		t->antwort_n += (size_t)n;
		t->antwort[t->antwort_n] = '\0';
	}
	closesocket(s);
	if (t->antwort == NULL || strncmp(t->antwort, "HTTP/", 5) != 0) {
		fehler(t, "keine HTTP-Antwort");
		return;
	}
	ende = strstr(t->antwort, "\r\n\r\n");
	if (ende == NULL) {
		fehler(t, "Kopf unvollständig");
		return;
	}
	p = strchr(t->antwort, ' ');
	status = p ? strtol(p + 1, NULL, 10) : 0;
	dazu(t, NS_T_STATUS, status, NULL, 0, NULL, 0);
	z = strstr(t->antwort, "\r\n") + 2;
	while (z < ende) {
		char *zende = strstr(z, "\r\n");
		dazu(t, NS_T_KOPF, 0, z, (size_t)(zende - z), NULL, 0);
		z = zende + 2;
	}
	dazu(t, NS_T_KOPF_ENDE, 0, NULL, 0, NULL, 0);
	p = ende + 4;
	while ((size_t)(p - t->antwort) < t->antwort_n) {
		size_t rest = t->antwort_n - (size_t)(p - t->antwort);
		size_t n = rest > 4096 ? 4096 : rest;
		dazu(t, NS_T_DATEN, 0, NULL, 0, (const uint8_t *)p, n);
		p += n;
	}
	dazu(t, NS_T_FERTIG, 0, NULL, 0, NULL, 0);
}

static char *kopie(const char *s)
{
	char *k;
	if (s == NULL)
		return NULL;
	k = malloc(strlen(s) + 1);
	if (k != NULL)
		strcpy(k, s);
	return k;
}

static void aufraeumen(struct ns_transport *t)
{
	int i;
	free(t->url);
	free(t->methode);
	free(t->kopfzeilen);
	free(t->agent);
	free(t->koerper);
	for (i = 0; i < t->n; i++)
		free(t->texte[i]);
	free(t->antwort);
	free(t);
}

static void loslassen(struct ns_transport *t)
{
	if (InterlockedDecrement(&t->besitzer) == 0)
		aufraeumen(t);
}

static DWORD WINAPI faden_lauf(LPVOID p)
{
	struct ns_transport *t = p;
	anfragen(t, t->url, t->methode, t->kopfzeilen, t->koerper, t->koerper_n, t->agent);
	InterlockedExchange(&t->fertig, 1);
	loslassen(t);
	return 0;
}

struct ns_transport *ns_transport_starten(const char *url, const char *methode, const char *kopfzeilen,
					  const void *koerper, size_t koerper_n, const char *agent)
{
	struct ns_transport *t = calloc(1, sizeof *t);
	if (t == NULL)
		return NULL;
	wsa_starten();
	/* Kopien: der Aufrufer gibt seine Texte nach dem Start frei */
	t->url = kopie(url);
	t->methode = kopie(methode);
	t->kopfzeilen = kopie(kopfzeilen);
	t->agent = kopie(agent);
	if (koerper != NULL && koerper_n > 0) {
		t->koerper = malloc(koerper_n);
		if (t->koerper != NULL)
			memcpy(t->koerper, koerper, koerper_n);
	}
	t->koerper_n = t->koerper != NULL ? koerper_n : 0;
	t->besitzer = 2;
	t->faden = CreateThread(NULL, 0, faden_lauf, t, 0, NULL);
	if (t->faden == NULL)
		faden_lauf(t); /* ohne Faden: wie bisher gleich durchlaufen */
	return t;
}

enum ns_transport_art ns_transport_holen(struct ns_transport *t, struct ns_transport_ereignis *e)
{
	if (InterlockedCompareExchange(&t->fertig, 0, 0) == 0) {
		memset(e, 0, sizeof *e);
		return NS_T_NICHTS;
	}
	if (t->naechstes >= t->n) {
		memset(e, 0, sizeof *e);
		return NS_T_NICHTS;
	}
	*e = t->e[t->naechstes++];
	return e->art;
}

void ns_transport_neue_sitzung(void)
{
	/* PC: Abrufe laufen im Browser-Faden, nichts überlebt das Programm */
}

void ns_transport_freigeben(struct ns_transport *t)
{
	/* läuft der Faden noch (Abbruch), gibt er am Ende selbst frei */
	if (t->faden != NULL)
		CloseHandle(t->faden);
	loslassen(t);
}
