/* ESP32-OS, Browser (B7): Prüfung von QuickJS-ng (PC-Tests): Skript in
 * einer eigenen Laufzeit mit Speichergrenze ausführen, Promise-Aufträge
 * abarbeiten, Ergebnis als Text (JSON bzw. Fehlermeldung). */
#include <stdio.h>
#include <string.h>
#include <time.h>
#include "quickjs.h"

static long ende_ms;

static long jetzt_ms(void)
{
	struct timespec t;
	timespec_get(&t, TIME_UTC);
	return (long)(t.tv_sec * 1000 + t.tv_nsec / 1000000);
}

/* Unterbrechung: Skripte, die die Zeitgrenze überschreiten (Endlosschleife) */
static int unterbrechen(JSRuntime *rt, void *pw)
{
	(void)rt;
	(void)pw;
	return jetzt_ms() > ende_ms;
}

/* Liefert 0 = Ergebnis, 1 = Ausnahme (Text in aus), < 0 = Fehler. */
int ns_probe_js(const char *code, size_t laenge, size_t speicher, int max_ms, char *aus, size_t max)
{
	JSRuntime *rt = JS_NewRuntime();
	JSContext *cx;
	JSValue v, txt;
	const char *s;
	int r = 0;
	if (rt == NULL)
		return -1;
	JS_SetMemoryLimit(rt, speicher);
	ende_ms = jetzt_ms() + max_ms;
	JS_SetInterruptHandler(rt, unterbrechen, NULL);
	cx = JS_NewContext(rt);
	if (cx == NULL) {
		JS_FreeRuntime(rt);
		return -2;
	}
	v = JS_Eval(cx, code, laenge, "<probe>", JS_EVAL_TYPE_GLOBAL);
	/* Promise-Aufträge (async/await) abarbeiten */
	for (;;) {
		JSContext *c2;
		int j = JS_ExecutePendingJob(rt, &c2);
		if (j <= 0)
			break;
	}
	if (JS_IsException(v)) {
		JSValue e = JS_GetException(cx);
		s = JS_ToCString(cx, e);
		snprintf(aus, max, "%s", s ? s : "(Ausnahme)");
		if (s)
			JS_FreeCString(cx, s);
		JS_FreeValue(cx, e);
		r = 1;
	} else {
		/* globalThis.ergebnis (nach den Aufträgen) bevorzugen */
		JSValue g = JS_GetGlobalObject(cx);
		JSValue erg = JS_GetPropertyStr(cx, g, "ergebnis");
		JSValue nehmen = JS_IsUndefined(erg) ? JS_DupValue(cx, v) : JS_DupValue(cx, erg);
		txt = JS_JSONStringify(cx, nehmen, JS_UNDEFINED, JS_UNDEFINED);
		s = JS_ToCString(cx, txt);
		snprintf(aus, max, "%s", s ? s : "undefined");
		if (s)
			JS_FreeCString(cx, s);
		JS_FreeValue(cx, txt);
		JS_FreeValue(cx, nehmen);
		JS_FreeValue(cx, erg);
		JS_FreeValue(cx, g);
	}
	JS_FreeValue(cx, v);
	JS_FreeContext(cx);
	JS_FreeRuntime(rt);
	return r;
}
