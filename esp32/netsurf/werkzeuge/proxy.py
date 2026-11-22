"""Proxy für die PC-Tests des Browsers (B2/B9): stellt echte Webseiten am PC
nach, obwohl der PC-Transport (netsurf/c/transport_pc.c) selbst kein TLS
kann.

Der PC-Transport schickt mit NS_PROXY=127.0.0.1:<port> jede http- und
https-Anfrage als „GET <ganze Adresse> HTTP/1.0“ hierher. Der Proxy holt die
Seite mit Python (TLS mit Zertifikatsprüfung), folgt keinen Weiterleitungen
(das macht NetSurf selbst), verlangt unkomprimierte Antworten und gibt
Status, Kopfzeilen und Inhalt zurück.

Mit --ablage ORDNER werden Antworten gespeichert und beim nächsten Mal von
dort geliefert (gleiche Seite, gleiche Bytes – wiederholbar); mit
--nur-ablage geht keine Anfrage mehr ins Netz.

    python -I esp32/netsurf/werkzeuge/proxy.py PORT [--ablage ORDNER] [--nur-ablage]

Lauscht nur auf 127.0.0.1.
"""
import hashlib
import http.server
import json
import os
import sys
import urllib.error
import urllib.request

WEG = {"connection", "keep-alive", "proxy-connection", "transfer-encoding", "content-encoding", "content-length",
       "upgrade", "te", "trailer", "host", "accept-encoding"}


class OhneWeiterleitung(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


OEFFNER = urllib.request.build_opener(OhneWeiterleitung)


def ablage_name(ordner, methode, url, koerper):
    h = hashlib.sha256(methode.encode() + b"\0" + url.encode() + b"\0" + koerper).hexdigest()[:32]
    return os.path.join(ordner, h)


class Proxy(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"
    ablage = None
    nur_ablage = False

    def log_message(self, fmt, *args):
        sys.stderr.write("proxy: " + (fmt % args) + "\n")

    def antworten(self, status, kopf, inhalt):
        self.send_response_only(status)
        for k, v in kopf:
            if k.lower() not in WEG:
                self.send_header(k, v)
        self.send_header("Content-Length", str(len(inhalt)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(inhalt)

    def holen(self, methode):
        url = self.path
        if not (url.startswith("http://") or url.startswith("https://")):
            self.antworten(400, [("Content-Type", "text/plain")], b"Proxy: ganze Adresse erwartet")
            return
        n = int(self.headers.get("Content-Length") or 0)
        koerper = self.rfile.read(n) if n > 0 else b""
        datei = ablage_name(self.ablage, methode, url, koerper) if self.ablage else None
        if datei and os.path.exists(datei + ".json"):
            with open(datei + ".json", encoding="utf-8") as f:
                m = json.load(f)
            with open(datei + ".bin", "rb") as f:
                inhalt = f.read()
            self.log_message("%s %s → %d (Ablage)", methode, url[:100], m["status"])
            self.antworten(m["status"], m["kopf"], inhalt)
            return
        if self.nur_ablage:
            self.antworten(504, [("Content-Type", "text/plain")], b"Proxy: nicht in der Ablage")
            return
        kopf = {k: v for k, v in self.headers.items() if k.lower() not in WEG}
        kopf["Accept-Encoding"] = "identity"
        anfrage = urllib.request.Request(url, data=koerper if methode == "POST" else None, headers=kopf, method=methode)
        try:
            with OEFFNER.open(anfrage, timeout=30) as r:
                status, rkopf, inhalt = r.status, list(r.headers.items()), r.read()
        except urllib.error.HTTPError as e:
            status, rkopf, inhalt = e.code, list(e.headers.items()), e.read()
        except Exception as e:  # Netz-/TLS-Fehler → 502 mit Grund
            self.log_message("%s %s → Fehler %s", methode, url[:100], e)
            self.antworten(502, [("Content-Type", "text/plain")], f"Proxy: {e}".encode())
            return
        if datei:
            with open(datei + ".json", "w", encoding="utf-8") as f:
                json.dump({"url": url, "status": status, "kopf": rkopf}, f)
            with open(datei + ".bin", "wb") as f:
                f.write(inhalt)
        self.log_message("%s %s → %d, %d Byte", methode, url[:100], status, len(inhalt))
        self.antworten(status, rkopf, inhalt)

    def do_GET(self):
        self.holen("GET")

    def do_POST(self):
        self.holen("POST")


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    port = int(args[0]) if args else 8899
    if "--ablage" in sys.argv:
        Proxy.ablage = sys.argv[sys.argv.index("--ablage") + 1]
        os.makedirs(Proxy.ablage, exist_ok=True)
    Proxy.nur_ablage = "--nur-ablage" in sys.argv
    server = http.server.ThreadingHTTPServer(("127.0.0.1", port), Proxy)
    sys.stderr.write(f"proxy: lausche auf 127.0.0.1:{port}\n")
    server.serve_forever()


if __name__ == "__main__":
    main()
