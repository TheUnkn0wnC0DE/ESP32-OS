// ESP32-OS, Browser (B7): DOM- und Web-Schicht für QuickJS-ng über den
// Grundfunktionen `__ns` (c/js_quickjs.c, libdom). Wird in jedem Dokument
// vor den Skripten der Seite ausgeführt.
(function () {
  'use strict';
  const N = globalThis.__ns;
  delete globalThis.__ns;
  const G = globalThis;
  const ELEMENT = 1, TEXT = 3, CDATA = 4, COMMENT = 8, DOCUMENT = 9, DOCTYPE = 10, FRAGMENT = 11;
  const HOERER = Symbol('hoerer');
  const ANGEMELDET = Symbol('angemeldet');
  const HANDLER = Symbol('handler');

  function melden(e) {
    N.protokoll('Fehler', (e && e.stack) ? `${e}\n${e.stack}` : String(e));
  }

  // ------------------------------------------------------------ Ereignisse
  // Hörer je Ziel: Map "typ|capture" → [{fn, once}]
  function hoererListe(ziel, typ, capture, anlegen) {
    let m = ziel[HOERER];
    if (!m) {
      if (!anlegen) return null;
      m = new Map();
      Object.defineProperty(ziel, HOERER, { value: m });
    }
    const k = `${typ}|${capture ? 1 : 0}`;
    let l = m.get(k);
    if (!l && anlegen) { l = []; m.set(k, l); }
    return l;
  }

  // Bei libdom je (Knoten, Typ) einmal für beide Phasen anmelden; Hörer
  // am Fenster laufen über das Dokument (Ende des libdom-Pfads).
  function anmelden(ziel, typ) {
    if (ziel === G) ziel = document;
    if (!(ziel instanceof Node)) return;
    let s = ziel[ANGEMELDET];
    if (!s) { s = new Set(); Object.defineProperty(ziel, ANGEMELDET, { value: s }); }
    if (s.has(typ)) return;
    const a = N.anmelden(ziel, typ, true);
    const b = N.anmelden(ziel, typ, false);
    if (a && b) s.add(typ);
  }

  function aufrufen(ziel, liste, ev) {
    if (!liste) return;
    for (const h of liste.slice()) {
      if (h.once) {
        const i = liste.indexOf(h);
        if (i >= 0) liste.splice(i, 1);
      }
      try {
        if (typeof h.fn === 'function') h.fn.call(ziel, ev);
        else if (h.fn && typeof h.fn.handleEvent === 'function') h.fn.handleEvent(ev);
      } catch (e) { melden(e); }
      if (ev[STOPP_SOFORT]) break;
    }
  }

  function handlerAufrufen(ziel, ev) {
    const f = ziel[HANDLER] && ziel[HANDLER][ev.type];
    if (typeof f === 'function') {
      try {
        const r = f.call(ziel, ev);
        if (r === false) ev.preventDefault();
      } catch (e) { melden(e); }
    }
  }

  const STOPP_SOFORT = Symbol('stoppSofort');

  class EventTarget {
    addEventListener(typ, fn, opt) {
      if (!fn) return;
      const capture = typeof opt === 'boolean' ? opt : !!(opt && opt.capture);
      const once = !!(opt && typeof opt === 'object' && opt.once);
      const l = hoererListe(this, String(typ), capture, true);
      if (l.some(h => h.fn === fn)) return;
      l.push({ fn, once });
      anmelden(this, String(typ));
    }
    removeEventListener(typ, fn, opt) {
      const capture = typeof opt === 'boolean' ? opt : !!(opt && opt.capture);
      const l = hoererListe(this, String(typ), capture, false);
      if (!l) return;
      const i = l.findIndex(h => h.fn === fn);
      if (i >= 0) l.splice(i, 1);
    }
    dispatchEvent(ev) {
      if (!(ev instanceof Event) && !(ev instanceof NativesEreignis)) throw new TypeError('dispatchEvent: kein Event');
      if (this === G || !(this instanceof Node)) {
        ev.target = this; ev.currentTarget = this; ev.eventPhase = 2;
        aufrufen(this, hoererListe(this, ev.type, true, false), ev);
        if (!ev[STOPP_SOFORT]) aufrufen(this, hoererListe(this, ev.type, false, false), ev);
        if (!ev[STOPP_SOFORT]) handlerAufrufen(this, ev);
        ev.currentTarget = null; ev.eventPhase = 0;
        return !ev.defaultPrevented;
      }
      ev.target = this;
      const ok = N.ausloesen(this, ev.type, !!ev.bubbles, !!ev.cancelable, ev);
      if (!ok && ev.cancelable) ev.defaultPrevented = true;
      return ok;
    }
  }

  // Ereignisse, die ein Skript selbst baut
  class Event {
    constructor(typ, init) {
      init = init || {};
      this.type = String(typ);
      this.bubbles = !!init.bubbles;
      this.cancelable = !!init.cancelable;
      this.composed = !!init.composed;
      this.defaultPrevented = false;
      this.timeStamp = performance.now();
      this.isTrusted = false;
      this.target = null;
      this.currentTarget = null;
      this.eventPhase = 0;
    }
    preventDefault() { if (this.cancelable) this.defaultPrevented = true; }
    stopPropagation() { this[STOPP] = true; }
    stopImmediatePropagation() { this[STOPP] = true; this[STOPP_SOFORT] = true; }
    composedPath() { return []; }
  }
  const STOPP = Symbol('stopp');
  Event.NONE = 0; Event.CAPTURING_PHASE = 1; Event.AT_TARGET = 2; Event.BUBBLING_PHASE = 3;
  class CustomEvent extends Event {
    constructor(typ, init) { super(typ, init); this.detail = init && 'detail' in init ? init.detail : null; }
  }
  class UIEvent extends Event { constructor(t, i) { super(t, i); this.detail = (i && i.detail) || 0; this.view = (i && i.view) || null; } }
  class MouseEvent extends UIEvent {
    constructor(t, i) {
      super(t, i); i = i || {};
      for (const k of ['clientX', 'clientY', 'screenX', 'screenY', 'pageX', 'pageY', 'button', 'buttons']) this[k] = i[k] || 0;
      for (const k of ['ctrlKey', 'shiftKey', 'altKey', 'metaKey']) this[k] = !!i[k];
      this.relatedTarget = i.relatedTarget || null;
    }
  }
  class KeyboardEvent extends UIEvent {
    constructor(t, i) {
      super(t, i); i = i || {};
      this.key = i.key || ''; this.code = i.code || ''; this.repeat = !!i.repeat;
      for (const k of ['ctrlKey', 'shiftKey', 'altKey', 'metaKey']) this[k] = !!i[k];
    }
  }
  class FocusEvent extends UIEvent {}
  class InputEvent extends UIEvent { constructor(t, i) { super(t, i); this.data = (i && i.data) || null; this.inputType = (i && i.inputType) || ''; } }

  // Hülle eines libdom-Ereignisses (Klasse aus C, Prototyp von hier)
  class NativesEreignis {
    get type() { return N.ereignis(this, 0); }
    get target() { return N.ereignis(this, 1); }
    get srcElement() { return N.ereignis(this, 1); }
    get currentTarget() { return this[AKTUELL] !== undefined ? this[AKTUELL] : N.ereignis(this, 2); }
    get eventPhase() { return N.ereignis(this, 3); }
    get bubbles() { return N.ereignis(this, 4); }
    get cancelable() { return N.ereignis(this, 5); }
    get defaultPrevented() { return N.ereignis(this, 6); }
    get isTrusted() { return true; }
    get timeStamp() { return performance.now(); }
    stopPropagation() { this[STOPP] = true; N.ereignis(this, 7); }
    stopImmediatePropagation() { this[STOPP] = true; this[STOPP_SOFORT] = true; N.ereignis(this, 8); }
    preventDefault() { N.ereignis(this, 9); }
    composedPath() { const p = []; for (let n = this.target; n; n = n.parentNode) p.push(n); p.push(G); return p; }
  }
  const AKTUELL = Symbol('aktuell');
  G.__ns_ereignis_proto = () => NativesEreignis.prototype;

  // Eigene Felder des Skript-Ereignisses (detail, key …) aufs native übertragen
  G.__ns_neues_ereignis = (nativ, eigen) => {
    if (!eigen || typeof eigen !== 'object') return;
    for (const k of Object.keys(eigen)) {
      if (['type', 'bubbles', 'cancelable', 'target', 'currentTarget', 'eventPhase', 'defaultPrevented', 'isTrusted', 'timeStamp'].includes(k)) continue;
      try { Object.defineProperty(nativ, k, { value: eigen[k], configurable: true, writable: true }); } catch (e) { melden(e); }
    }
  };

  // Fenster als Ende des Ereignispfads (libdom endet am Dokument)
  function fensterAufrufen(ev, phase) {
    const skript = ev instanceof Event;
    const alt = skript ? ev.currentTarget : ev[AKTUELL];
    if (skript) ev.currentTarget = G; else ev[AKTUELL] = G;
    if (phase === 1 || phase === 2) aufrufen(G, hoererListe(G, ev.type, true, false), ev);
    if (phase === 2 || phase === 3) {
      if (!ev[STOPP_SOFORT]) aufrufen(G, hoererListe(G, ev.type, false, false), ev);
      if (!ev[STOPP_SOFORT]) handlerAufrufen(G, ev);
    }
    if (skript) ev.currentTarget = alt; else ev[AKTUELL] = alt;
  }

  // Von C (libdom-Hörer) je Knoten und Phase gerufen. libdom führt das
  // Ziel selbst auch im Fang- und Sprudelpfad (Phase 1 und 3) und ruft es
  // in Phase 2 je Anmeldung; ausserdem sprudelt libdom auch Ereignisse mit
  // bubbles = false. Hier wird das auf die DOM-Regeln gebracht.
  const AM_ZIEL = Symbol('amZiel');
  G.__ns_feuern = (knoten, ev, phase) => {
    if (knoten === ev.target) {
      if (phase !== 2 || ev[AM_ZIEL]) return;
      Object.defineProperty(ev, AM_ZIEL, { value: true });
      if (knoten instanceof Document) fensterAufrufen(ev, 1);
      if (ev[STOPP]) return;
      aufrufen(knoten, hoererListe(knoten, ev.type, true, false), ev);
      if (!ev[STOPP_SOFORT]) aufrufen(knoten, hoererListe(knoten, ev.type, false, false), ev);
      if (!ev[STOPP_SOFORT]) handlerAufrufen(knoten, ev);
      if (knoten instanceof Document && ev.bubbles && !ev[STOPP]) fensterAufrufen(ev, 3);
      return;
    }
    if (phase === 1) {
      if (knoten instanceof Document) fensterAufrufen(ev, 1);
      if (!ev[STOPP]) aufrufen(knoten, hoererListe(knoten, ev.type, true, false), ev);
    } else if (phase === 3 && ev.bubbles) {
      aufrufen(knoten, hoererListe(knoten, ev.type, false, false), ev);
      if (!ev[STOPP_SOFORT]) handlerAufrufen(knoten, ev);
      if (knoten instanceof Document && !ev[STOPP]) fensterAufrufen(ev, 3);
    }
  };

  // on<typ>-Eigenschaften
  const ON = ['click', 'dblclick', 'mousedown', 'mouseup', 'mouseover', 'mouseout', 'mousemove', 'mouseenter', 'mouseleave',
    'keydown', 'keyup', 'keypress', 'input', 'change', 'submit', 'reset', 'focus', 'blur', 'load', 'error', 'scroll', 'resize',
    'touchstart', 'touchend', 'touchmove', 'pointerdown', 'pointerup', 'pointermove', 'select', 'contextmenu', 'wheel'];
  function onDefinieren(proto) {
    for (const typ of ON) {
      Object.defineProperty(proto, 'on' + typ, {
        configurable: true,
        get() { return (this[HANDLER] && this[HANDLER][typ]) || null; },
        set(f) {
          if (!this[HANDLER]) Object.defineProperty(this, HANDLER, { value: Object.create(null) });
          this[HANDLER][typ] = typeof f === 'function' ? f : null;
          anmelden(this, typ);
        },
      });
    }
  }

  // ------------------------------------------------------------ Selektoren
  // Unterstützt: * typ #id .klasse [a] [a=w] [a~=w] [a|=w] [a^=w] [a$=w]
  // [a*=w] :first-child :last-child :only-child :nth-child() :nth-last-child()
  // :first-of-type :last-of-type :not() :checked :disabled :enabled :empty
  // :root, Kombinatoren " " > + ~ und Listen mit ",".
  const selCache = new Map();
  function selParsen(text) {
    let c = selCache.get(text);
    if (c) return c;
    let i = 0;
    const s = text.trim();
    const liste = [];
    function leer() { while (i < s.length && /\s/.test(s[i])) i++; }
    function name() {
      let n = '';
      while (i < s.length && /[\w\- -￿\\]/.test(s[i])) {
        if (s[i] === '\\') { n += s[i + 1]; i += 2; } else n += s[i++];
      }
      return n;
    }
    function wert() {
      leer();
      if (s[i] === '"' || s[i] === "'") {
        const q = s[i++];
        let w = '';
        while (i < s.length && s[i] !== q) { if (s[i] === '\\') i++; w += s[i++]; }
        i++;
        return w;
      }
      return name();
    }
    function nth(arg) {
      arg = arg.replace(/\s+/g, '').toLowerCase();
      if (arg === 'odd') return [2, 1];
      if (arg === 'even') return [2, 0];
      const m = /^([+-]?\d*)n([+-]\d+)?$/.exec(arg);
      if (m) return [m[1] === '' || m[1] === '+' ? 1 : m[1] === '-' ? -1 : +m[1], m[2] ? +m[2] : 0];
      return [0, +arg];
    }
    function einfach() {
      const t = { tag: null, id: null, klassen: [], attr: [], pseudo: [] };
      let irgendwas = false;
      for (;;) {
        if (s[i] === '*') { i++; irgendwas = true; continue; }
        if (s[i] === '#') { i++; t.id = name(); irgendwas = true; continue; }
        if (s[i] === '.') { i++; t.klassen.push(name()); irgendwas = true; continue; }
        if (s[i] === '[') {
          i++; leer();
          const a = name().toLowerCase();
          leer();
          let op = null, w = null;
          if (s[i] === ']') { i++; } else {
            op = s[i] === '=' ? '=' : s.substr(i, 2);
            i += op.length;
            w = wert();
            leer();
            if (/[is]/i.test(s[i]) && s[i + 1] === ']') i++;
            if (s[i] === ']') i++;
          }
          t.attr.push([a, op, w]);
          irgendwas = true;
          continue;
        }
        if (s[i] === ':') {
          i++;
          if (s[i] === ':') i++;
          const p = name().toLowerCase();
          let arg = null;
          if (s[i] === '(') {
            let tiefe = 1, a = '';
            i++;
            while (i < s.length && tiefe > 0) {
              if (s[i] === '(') tiefe++;
              if (s[i] === ')') { tiefe--; if (tiefe === 0) break; }
              a += s[i++];
            }
            i++;
            arg = a;
          }
          if (p === 'not' || p === 'is' || p === 'where') t.pseudo.push([p, selParsen(arg)]);
          else if (p.startsWith('nth')) t.pseudo.push([p, nth(arg)]);
          else t.pseudo.push([p, arg]);
          irgendwas = true;
          continue;
        }
        if (i < s.length && /[\w\- -￿]/.test(s[i])) { t.tag = name().toUpperCase(); irgendwas = true; continue; }
        break;
      }
      if (!irgendwas) throw new SyntaxError(`Selektor nicht verstanden: ${text}`);
      return t;
    }
    for (;;) {
      leer();
      const kette = [{ komb: null, t: einfach() }];
      for (;;) {
        const vor = i;
        leer();
        if (i >= s.length || s[i] === ',') break;
        let komb = ' ';
        if ('>+~'.includes(s[i])) { komb = s[i++]; leer(); } else if (i === vor) break;
        kette.push({ komb, t: einfach() });
      }
      liste.push(kette);
      leer();
      if (s[i] === ',') { i++; continue; }
      if (i < s.length) throw new SyntaxError(`Selektor nicht verstanden: ${text}`);
      break;
    }
    selCache.set(text, liste);
    return liste;
  }

  function geschwisterIndex(el, gleicherTyp, vonHinten) {
    let n = 1;
    for (let x = vonHinten ? el.nextSibling : el.previousSibling; x; x = vonHinten ? x.nextSibling : x.previousSibling)
      if (x.nodeType === ELEMENT && (!gleicherTyp || x.nodeName === el.nodeName)) n++;
    return n;
  }
  function passtNth([a, b], i) { return a === 0 ? i === b : (i - b) / a >= 0 && (i - b) % a === 0; }

  function passtEinfach(el, t) {
    if (t.tag && t.tag !== '*' && el.nodeName !== t.tag) return false;
    if (t.id && el.getAttribute('id') !== t.id) return false;
    if (t.klassen.length) {
      const k = (el.getAttribute('class') || '').split(/\s+/);
      for (const c of t.klassen) if (!k.includes(c)) return false;
    }
    for (const [a, op, w] of t.attr) {
      const v = el.getAttribute(a);
      if (v === null) return false;
      if (op === null) continue;
      if (op === '=' && v !== w) return false;
      if (op === '~=' && !v.split(/\s+/).includes(w)) return false;
      if (op === '|=' && v !== w && !v.startsWith(w + '-')) return false;
      if (op === '^=' && !(w && v.startsWith(w))) return false;
      if (op === '$=' && !(w && v.endsWith(w))) return false;
      if (op === '*=' && !(w && v.includes(w))) return false;
    }
    for (const [p, arg] of t.pseudo) {
      switch (p) {
        case 'not': if (passtListe(el, arg)) return false; break;
        case 'is': case 'where': if (!passtListe(el, arg)) return false; break;
        case 'first-child': if (el.previousElementSibling) return false; break;
        case 'last-child': if (el.nextElementSibling) return false; break;
        case 'only-child': if (el.previousElementSibling || el.nextElementSibling) return false; break;
        case 'first-of-type': if (geschwisterIndex(el, true, false) !== 1) return false; break;
        case 'last-of-type': if (geschwisterIndex(el, true, true) !== 1) return false; break;
        case 'nth-child': if (!passtNth(arg, geschwisterIndex(el, false, false))) return false; break;
        case 'nth-last-child': if (!passtNth(arg, geschwisterIndex(el, false, true))) return false; break;
        case 'nth-of-type': if (!passtNth(arg, geschwisterIndex(el, true, false))) return false; break;
        case 'checked': if (el.getAttribute('checked') === null && el.getAttribute('selected') === null) return false; break;
        case 'disabled': if (el.getAttribute('disabled') === null) return false; break;
        case 'enabled': if (el.getAttribute('disabled') !== null) return false; break;
        case 'empty': if (el.firstChild) return false; break;
        case 'root': if (el.parentNode !== document) return false; break;
        case 'link': case 'any-link': if (el.nodeName !== 'A' || el.getAttribute('href') === null) return false; break;
        case 'hover': case 'focus': case 'active': case 'visited': case 'focus-within': case 'focus-visible': return false;
        default: return false;
      }
    }
    return true;
  }

  function passtKette(el, kette, k) {
    if (!passtEinfach(el, kette[k].t)) return false;
    if (k === 0) return true;
    const komb = kette[k].komb;
    if (komb === '>') { const p = el.parentNode; return !!p && p.nodeType === ELEMENT && passtKette(p, kette, k - 1); }
    if (komb === '+') { const p = el.previousElementSibling; return !!p && passtKette(p, kette, k - 1); }
    if (komb === '~') { for (let p = el.previousElementSibling; p; p = p.previousElementSibling) if (passtKette(p, kette, k - 1)) return true; return false; }
    for (let p = el.parentNode; p && p.nodeType === ELEMENT; p = p.parentNode) if (passtKette(p, kette, k - 1)) return true;
    return false;
  }
  function passtListe(el, liste) { return liste.some(kette => passtKette(el, kette, kette.length - 1)); }

  function* elemente(wurzel) {
    // Tiefensuche ohne Rekursion
    let n = wurzel.firstChild;
    while (n) {
      if (n.nodeType === ELEMENT) yield n;
      if (n.firstChild) { n = n.firstChild; continue; }
      while (n && !n.nextSibling) { n = n.parentNode; if (n === wurzel) return; }
      if (n) n = n.nextSibling;
    }
  }
  function abfragen(wurzel, sel, alle) {
    const liste = selParsen(String(sel));
    const r = [];
    for (const el of elemente(wurzel)) {
      if (passtListe(el, liste)) { if (!alle) return el; r.push(el); }
    }
    return alle ? r : null;
  }

  // ------------------------------------------------------------ Serialisieren
  const LEER = new Set(['AREA', 'BASE', 'BR', 'COL', 'EMBED', 'HR', 'IMG', 'INPUT', 'LINK', 'META', 'PARAM', 'SOURCE', 'TRACK', 'WBR']);
  const ROH = new Set(['SCRIPT', 'STYLE', 'XMP', 'IFRAME', 'NOEMBED', 'NOFRAMES', 'PLAINTEXT', 'NOSCRIPT']);
  const esc = (t, attr) => t.replace(/&/g, '&amp;').replace(/ /g, '&nbsp;')
    .replace(attr ? /"/g : /</g, attr ? '&quot;' : '&lt;').replace(attr ? /\u0000/g : />/g, attr ? '' : '&gt;');
  function serialisieren(n, eltern) {
    switch (n.nodeType) {
      case ELEMENT: {
        const tag = n.nodeName.toLowerCase();
        let s = '<' + tag;
        for (const [k, v] of N.attrs(n)) s += ` ${k}="${esc(v, true)}"`;
        s += '>';
        if (LEER.has(n.nodeName)) return s;
        for (let c = n.firstChild; c; c = c.nextSibling) s += serialisieren(c, n);
        return s + `</${tag}>`;
      }
      case TEXT: case CDATA: {
        const t = n.nodeValue || '';
        return eltern && ROH.has(eltern.nodeName) ? t : esc(t, false);
      }
      case COMMENT: return `<!--${n.nodeValue || ''}-->`;
      case DOCTYPE: return `<!DOCTYPE ${n.nodeName}>`;
      default: { let s = ''; for (let c = n.firstChild; c; c = c.nextSibling) s += serialisieren(c, n); return s; }
    }
  }

  // ------------------------------------------------------------ Knoten
  function knotenAus(x) {
    return x instanceof Node ? x : document.createTextNode(String(x));
  }

  class NodeList extends Array {
    item(i) { return this[i] ?? null; }
  }
  function nodeList(a) { const l = new NodeList(); for (const x of a) l.push(x); return l; }

  class Node extends EventTarget {
    get nodeType() { return N.typ(this); }
    get nodeName() { return N.name(this); }
    get nodeValue() { return N.wert(this); }
    set nodeValue(v) { N.wertSetzen(this, v === null ? '' : String(v)); }
    get textContent() { const t = this.nodeType; return t === DOCUMENT || t === DOCTYPE ? null : N.text(this); }
    set textContent(v) { N.textSetzen(this, v === null || v === undefined ? '' : String(v)); }
    get parentNode() { return N.nachbar(this, 0); }
    get parentElement() { const p = this.parentNode; return p && p.nodeType === ELEMENT ? p : null; }
    get firstChild() { return N.nachbar(this, 1); }
    get lastChild() { return N.nachbar(this, 2); }
    get nextSibling() { return N.nachbar(this, 3); }
    get previousSibling() { return N.nachbar(this, 4); }
    get childNodes() { const a = []; for (let c = this.firstChild; c; c = c.nextSibling) a.push(c); return nodeList(a); }
    get ownerDocument() { return this.nodeType === DOCUMENT ? null : document; }
    get isConnected() { for (let n = this; n; n = n.parentNode) if (n.nodeType === DOCUMENT) return true; return false; }
    hasChildNodes() { return this.firstChild !== null; }
    appendChild(c) { N.baum(0, this, c); return c; }
    insertBefore(c, ref) { if (ref) N.baum(1, this, c, ref); else N.baum(0, this, c); return c; }
    removeChild(c) { N.baum(2, this, c); return c; }
    replaceChild(neu, alt) { N.baum(3, this, neu, alt); return alt; }
    cloneNode(tief) { return N.klonen(this, !!tief); }
    contains(o) { for (; o; o = o.parentNode) if (o === this) return true; return false; }
    isSameNode(o) { return o === this; }
    getRootNode() { let n = this; while (n.parentNode) n = n.parentNode; return n; }
    compareDocumentPosition(o) { return this.contains(o) ? 20 : o.contains(this) ? 10 : 4; }
    normalize() { /* libdom hält Text bereits zusammen, wo es zählt */ }
  }
  Node.ELEMENT_NODE = ELEMENT; Node.TEXT_NODE = TEXT; Node.COMMENT_NODE = COMMENT;
  Node.DOCUMENT_NODE = DOCUMENT; Node.DOCUMENT_FRAGMENT_NODE = FRAGMENT; Node.DOCUMENT_TYPE_NODE = DOCTYPE;

  // ParentNode/ChildNode-Ergänzungen
  const Eltern = {
    get children() { const a = []; for (let c = this.firstChild; c; c = c.nextSibling) if (c.nodeType === ELEMENT) a.push(c); return nodeList(a); },
    get childElementCount() { return this.children.length; },
    get firstElementChild() { for (let c = this.firstChild; c; c = c.nextSibling) if (c.nodeType === ELEMENT) return c; return null; },
    get lastElementChild() { for (let c = this.lastChild; c; c = c.previousSibling) if (c.nodeType === ELEMENT) return c; return null; },
    append(...x) { for (const n of x) this.appendChild(knotenAus(n)); },
    prepend(...x) { const f = this.firstChild; for (const n of x) this.insertBefore(knotenAus(n), f); },
    replaceChildren(...x) { while (this.firstChild) this.removeChild(this.firstChild); this.append(...x); },
    querySelector(s) { return abfragen(this, s, false); },
    querySelectorAll(s) { return nodeList(abfragen(this, s, true)); },
    getElementsByTagName(t) { t = String(t).toUpperCase(); return nodeList(t === '*' ? [...elemente(this)] : [...elemente(this)].filter(e => e.nodeName === t)); },
    getElementsByClassName(k) { const ks = String(k).trim().split(/\s+/); return nodeList([...elemente(this)].filter(e => { const c = (e.getAttribute('class') || '').split(/\s+/); return ks.every(x => c.includes(x)); })); },
  };
  const Kind = {
    get nextElementSibling() { for (let c = this.nextSibling; c; c = c.nextSibling) if (c.nodeType === ELEMENT) return c; return null; },
    get previousElementSibling() { for (let c = this.previousSibling; c; c = c.previousSibling) if (c.nodeType === ELEMENT) return c; return null; },
    remove() { const p = this.parentNode; if (p) p.removeChild(this); },
    before(...x) { const p = this.parentNode; if (p) for (const n of x) p.insertBefore(knotenAus(n), this); },
    after(...x) { const p = this.parentNode; if (!p) return; const r = this.nextSibling; for (const n of x) p.insertBefore(knotenAus(n), r); },
    replaceWith(...x) { const p = this.parentNode; if (!p) return; const r = this.nextSibling; p.removeChild(this); for (const n of x) p.insertBefore(knotenAus(n), r); },
  };
  function mischen(ziel, quelle) { Object.defineProperties(ziel, Object.getOwnPropertyDescriptors(quelle)); }

  class CharacterData extends Node {
    get data() { return this.nodeValue; }
    set data(v) { this.nodeValue = v; }
    get length() { return (this.nodeValue || '').length; }
    appendData(d) { this.data += d; }
  }
  class Text extends CharacterData {
    get wholeText() { return this.data; }
    splitText(o) { const d = this.data; this.data = d.slice(0, o); const t = document.createTextNode(d.slice(o)); this.after(t); return t; }
  }
  class Comment extends CharacterData {}
  mischen(CharacterData.prototype, Kind);

  // DOMTokenList über ein Attribut
  class DOMTokenList {
    constructor(el, attr) { this._el = el; this._a = attr; }
    _liste() { return (this._el.getAttribute(this._a) || '').split(/\s+/).filter(Boolean); }
    _setzen(l) { this._el.setAttribute(this._a, l.join(' ')); }
    get length() { return this._liste().length; }
    get value() { return this._el.getAttribute(this._a) || ''; }
    item(i) { return this._liste()[i] ?? null; }
    contains(t) { return this._liste().includes(t); }
    add(...t) { const l = this._liste(); for (const x of t) if (!l.includes(x)) l.push(x); this._setzen(l); }
    remove(...t) { this._setzen(this._liste().filter(x => !t.includes(x))); }
    toggle(t, erzwingen) {
      const da = this.contains(t);
      const soll = erzwingen === undefined ? !da : !!erzwingen;
      if (soll && !da) this.add(t); else if (!soll && da) this.remove(t);
      return soll;
    }
    replace(a, b) { const l = this._liste(); const i = l.indexOf(a); if (i < 0) return false; l[i] = b; this._setzen(l); return true; }
    forEach(f, t) { this._liste().forEach(f, t); }
    [Symbol.iterator]() { return this._liste()[Symbol.iterator](); }
    toString() { return this.value; }
  }

  // style: Deklarationen des style-Attributs
  const zuKebab = n => n.startsWith('--') ? n : n.replace(/[A-Z]/g, m => '-' + m.toLowerCase()).replace(/^(webkit|moz|ms)-/, '-$1-');
  function stilLesen(el) {
    const m = new Map();
    for (const d of (el.getAttribute('style') || '').split(';')) {
      const i = d.indexOf(':');
      if (i > 0) m.set(d.slice(0, i).trim().toLowerCase(), d.slice(i + 1).trim());
    }
    return m;
  }
  function stilSchreiben(el, m) {
    el.setAttribute('style', [...m].map(([k, v]) => `${k}: ${v}`).join('; '));
  }
  function stilObjekt(el) {
    const ziel = {
      getPropertyValue(n) { return stilLesen(el).get(zuKebab(n)) || ''; },
      setProperty(n, v) { const m = stilLesen(el); const k = zuKebab(n); if (v === null || v === '') m.delete(k); else m.set(k, String(v)); stilSchreiben(el, m); },
      removeProperty(n) { const m = stilLesen(el); const k = zuKebab(n); const alt = m.get(k) || ''; m.delete(k); stilSchreiben(el, m); return alt; },
      get cssText() { return el.getAttribute('style') || ''; },
      set cssText(t) { el.setAttribute('style', String(t)); },
      get length() { return stilLesen(el).size; },
      item(i) { return [...stilLesen(el).keys()][i] || ''; },
    };
    return new Proxy(ziel, {
      get(t, k) { return k in t || typeof k === 'symbol' ? t[k] : t.getPropertyValue(k); },
      set(t, k, v) { if (k === 'cssText') t.cssText = v; else t.setProperty(k, v); return true; },
    });
  }
  function datasetObjekt(el) {
    const zuAttr = k => 'data-' + k.replace(/[A-Z]/g, m => '-' + m.toLowerCase());
    return new Proxy({}, {
      get(_, k) { if (typeof k !== 'string') return undefined; const v = el.getAttribute(zuAttr(k)); return v === null ? undefined : v; },
      set(_, k, v) { el.setAttribute(zuAttr(String(k)), String(v)); return true; },
      deleteProperty(_, k) { el.removeAttribute(zuAttr(String(k))); return true; },
      has(_, k) { return el.getAttribute(zuAttr(String(k))) !== null; },
      ownKeys() { return N.attrs(el).map(([k]) => k).filter(k => k.startsWith('data-')).map(k => k.slice(5).replace(/-([a-z])/g, (m, c) => c.toUpperCase())); },
      getOwnPropertyDescriptor(_, k) { const v = el.getAttribute(zuAttr(String(k))); return v === null ? undefined : { value: v, enumerable: true, configurable: true, writable: true }; },
    });
  }

  class Element extends Node {
    get tagName() { return this.nodeName; }
    get localName() { return this.nodeName.toLowerCase(); }
    get namespaceURI() { return 'http://www.w3.org/1999/xhtml'; }
    getAttribute(n) { return N.attr(this, String(n).toLowerCase()); }
    setAttribute(n, v) { N.attrSetzen(this, String(n).toLowerCase(), String(v)); }
    removeAttribute(n) { N.attrSetzen(this, String(n).toLowerCase()); }
    hasAttribute(n) { return this.getAttribute(n) !== null; }
    hasAttributes() { return N.attrs(this).length > 0; }
    toggleAttribute(n, erzwingen) {
      const da = this.hasAttribute(n);
      const soll = erzwingen === undefined ? !da : !!erzwingen;
      if (soll && !da) this.setAttribute(n, ''); else if (!soll && da) this.removeAttribute(n);
      return soll;
    }
    getAttributeNames() { return N.attrs(this).map(([k]) => k); }
    get attributes() { const a = N.attrs(this).map(([name, value]) => ({ name, value, nodeName: name, nodeValue: value })); a.getNamedItem = n => a.find(x => x.name === n) || null; return a; }
    get id() { return this.getAttribute('id') || ''; }
    set id(v) { this.setAttribute('id', v); }
    get className() { return this.getAttribute('class') || ''; }
    set className(v) { this.setAttribute('class', v); }
    get classList() { return new DOMTokenList(this, 'class'); }
    get innerHTML() { let s = ''; for (let c = this.firstChild; c; c = c.nextSibling) s += serialisieren(c, this); return s; }
    set innerHTML(h) {
      while (this.firstChild) this.removeChild(this.firstChild);
      if (h !== null && h !== undefined && h !== '') this.appendChild(N.fragment(String(h), this.nodeName));
    }
    get outerHTML() { return serialisieren(this, this.parentNode); }
    set outerHTML(h) { const p = this.parentNode; if (!p) return; p.insertBefore(N.fragment(String(h), p.nodeName), this); p.removeChild(this); }
    insertAdjacentHTML(wo, h) {
      const aussen = /^(beforebegin|afterend)$/i.test(wo);
      const kontext = aussen ? (this.parentNode ? this.parentNode.nodeName : 'BODY') : this.nodeName;
      this.insertAdjacentElement(wo, N.fragment(String(h), kontext));
    }
    insertAdjacentText(wo, t) { this.insertAdjacentElement(wo, document.createTextNode(String(t))); }
    insertAdjacentElement(wo, n) {
      switch (String(wo).toLowerCase()) {
        case 'beforebegin': if (this.parentNode) this.parentNode.insertBefore(n, this); break;
        case 'afterbegin': this.insertBefore(n, this.firstChild); break;
        case 'beforeend': this.appendChild(n); break;
        case 'afterend': if (this.parentNode) this.parentNode.insertBefore(n, this.nextSibling); break;
        default: throw new SyntaxError('insertAdjacent: ' + wo);
      }
      return n;
    }
    get style() { return stilObjekt(this); }
    set style(t) { this.setAttribute('style', String(t)); }
    get dataset() { return datasetObjekt(this); }
    matches(s) { return passtListe(this, selParsen(String(s))); }
    closest(s) { const l = selParsen(String(s)); for (let e = this; e && e.nodeType === ELEMENT; e = e.parentNode) if (passtListe(e, l)) return e; return null; }
    get innerText() { return this.textContent; }
    set innerText(v) { this.textContent = v; }
    get outerText() { return this.textContent; }
    get hidden() { return this.hasAttribute('hidden'); }
    set hidden(v) { this.toggleAttribute('hidden', !!v); }
    get title() { return this.getAttribute('title') || ''; }
    set title(v) { this.setAttribute('title', v); }
    get lang() { return this.getAttribute('lang') || ''; }
    get dir() { return this.getAttribute('dir') || ''; }
    get tabIndex() { return +(this.getAttribute('tabindex') || -1); }
    click() { this.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true })); }
    focus() { this.dispatchEvent(new FocusEvent('focus')); }
    blur() { this.dispatchEvent(new FocusEvent('blur')); }
    // Ohne Layout-Rückmeldung (siehe STATUS): Maße 0
    getBoundingClientRect() { return { x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 }; }
    getClientRects() { return []; }
    get offsetWidth() { return 0; } get offsetHeight() { return 0; } get offsetTop() { return 0; } get offsetLeft() { return 0; }
    get clientWidth() { return 0; } get clientHeight() { return 0; } get scrollWidth() { return 0; } get scrollHeight() { return 0; }
    scrollIntoView() {} // Blättern übernimmt NetSurf nicht für Skripte
  }
  mischen(Element.prototype, Eltern);
  mischen(Element.prototype, Kind);
  onDefinieren(Element.prototype);

  // HTML-Eigenschaften, die Seiten oft direkt lesen/setzen
  function attrEigenschaft(proto, name, attr, art) {
    Object.defineProperty(proto, name, {
      configurable: true,
      get() {
        const v = this.getAttribute(attr);
        if (art === 'bool') return v !== null;
        if (art === 'url') return v === null ? '' : new URL(v, document.baseURI).href;
        if (art === 'zahl') return v === null ? 0 : +v;
        return v === null ? '' : v;
      },
      set(v) {
        if (art === 'bool') this.toggleAttribute(attr, !!v);
        else this.setAttribute(attr, v);
      },
    });
  }
  class HTMLElement extends Element {}
  const P = HTMLElement.prototype;
  for (const [n, a, t] of [['href', 'href', 'url'], ['src', 'src', 'url'], ['action', 'action', 'url'], ['name', 'name'], ['type', 'type'],
    ['alt', 'alt'], ['rel', 'rel'], ['target', 'target'], ['placeholder', 'placeholder'], ['htmlFor', 'for'], ['checked', 'checked', 'bool'],
    ['disabled', 'disabled', 'bool'], ['selected', 'selected', 'bool'], ['readOnly', 'readonly', 'bool'], ['required', 'required', 'bool'],
    ['multiple', 'multiple', 'bool'], ['width', 'width', 'zahl'], ['height', 'height', 'zahl'], ['method', 'method'], ['content', 'content'],
    ['defaultValue', 'value']]) attrEigenschaft(P, n, a, t);
  Object.defineProperty(P, 'value', {
    configurable: true,
    get() {
      if (this.nodeName === 'TEXTAREA') return this.textContent;
      if (this.nodeName === 'SELECT') { const o = this.querySelector('option[selected]') || this.querySelector('option'); return o ? (o.getAttribute('value') ?? o.textContent) : ''; }
      return this.getAttribute('value') ?? (this.nodeName === 'OPTION' ? this.textContent : '');
    },
    set(v) {
      if (this.nodeName === 'TEXTAREA') this.textContent = v;
      else if (this.nodeName === 'SELECT') { for (const o of this.querySelectorAll('option')) o.toggleAttribute('selected', (o.getAttribute('value') ?? o.textContent) === String(v)); }
      else this.setAttribute('value', v);
    },
  });
  Object.defineProperty(P, 'options', { configurable: true, get() { return this.querySelectorAll('option'); } });
  Object.defineProperty(P, 'elements', { configurable: true, get() { return this.querySelectorAll('input,select,textarea,button'); } });
  Object.defineProperty(P, 'form', { configurable: true, get() { return this.closest('form'); } });
  P.submit = function () { this.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); };
  P.reset = function () { this.dispatchEvent(new Event('reset', { bubbles: true, cancelable: true })); };
  P.select = function () {};

  class DocumentFragment extends Node {}
  mischen(DocumentFragment.prototype, Eltern);
  DocumentFragment.prototype.getElementById = function (id) { return this.querySelector('#' + CSS.escape(id)); };

  class DocumentType extends Node { get name() { return this.nodeName; } }

  let bereit = 'loading';
  const cookies = new Map();
  class Document extends Node {
    get documentElement() { return this.firstElementChild; }
    get head() { return this.querySelector('head'); }
    get body() { return this.querySelector('body'); }
    get title() { const t = this.querySelector('title'); return t ? t.textContent.trim() : ''; }
    set title(v) { let t = this.querySelector('title'); if (!t) { t = this.createElement('title'); (this.head || this.documentElement).appendChild(t); } t.textContent = v; }
    get URL() { return N.adresse(); }
    get documentURI() { return N.adresse(); }
    get baseURI() { const b = this.querySelector('base[href]'); return b ? new URL(b.getAttribute('href'), N.adresse()).href : N.adresse(); }
    get location() { return location; }
    set location(v) { location.href = v; }
    get defaultView() { return G; }
    get readyState() { return bereit; }
    get characterSet() { return 'UTF-8'; }
    get charset() { return 'UTF-8'; }
    get contentType() { return 'text/html'; }
    get compatMode() { return 'CSS1Compat'; }
    get visibilityState() { return 'visible'; }
    get hidden() { return false; }
    get activeElement() { return this.body; }
    get referrer() { return ''; }
    get domain() { try { return new URL(N.adresse()).hostname; } catch (e) { return ''; } }
    get cookie() { return [...cookies].map(([k, v]) => `${k}=${v}`).join('; '); }
    set cookie(s) { const [kv] = String(s).split(';'); const i = kv.indexOf('='); if (i > 0) cookies.set(kv.slice(0, i).trim(), kv.slice(i + 1).trim()); }
    get forms() { return this.querySelectorAll('form'); }
    get images() { return this.querySelectorAll('img'); }
    get links() { return this.querySelectorAll('a[href],area[href]'); }
    get scripts() { return this.querySelectorAll('script'); }
    get styleSheets() { return []; }
    getElementById(id) { return N.nachId(String(id)); }
    getElementsByName(n) { return this.querySelectorAll(`[name="${String(n).replace(/"/g, '\\"')}"]`); }
    createElement(t) { return N.erzeugen(0, String(t).toLowerCase()); }
    createElementNS(ns, t) { return this.createElement(String(t).split(':').pop()); }
    createTextNode(t) { return N.erzeugen(1, String(t)); }
    createComment(t) { return N.erzeugen(2, String(t)); }
    createDocumentFragment() { return N.erzeugen(3); }
    createEvent(t) { return new (/Mouse/.test(t) ? MouseEvent : /Key/.test(t) ? KeyboardEvent : /Custom/.test(t) ? CustomEvent : Event)(''); }
    hasFocus() { return true; }
    write(...t) { const b = this.body || this.documentElement; if (b) b.insertAdjacentHTML('beforeend', t.join('')); }
    writeln(...t) { this.write(...t, '\n'); }
    open() { return this; }
    close() {}
    elementFromPoint() { return null; }
  }
  mischen(Document.prototype, Eltern);
  onDefinieren(Document.prototype);
  Event.prototype.initEvent = function (t, b, c) { this.type = String(t); this.bubbles = !!b; this.cancelable = !!c; };
  CustomEvent.prototype.initCustomEvent = function (t, b, c, d) { this.initEvent(t, b, c); this.detail = d; };

  G.__ns_proto = (typ, name) => {
    switch (typ) {
      case ELEMENT: return HTMLElement.prototype;
      case TEXT: case CDATA: return Text.prototype;
      case COMMENT: return Comment.prototype;
      case DOCUMENT: return Document.prototype;
      case DOCTYPE: return DocumentType.prototype;
      case FRAGMENT: return DocumentFragment.prototype;
      default: return Node.prototype;
    }
  };

  const document = N.dokument();

  // Inline-Handler (onclick="…") beim Einfügen eines Elements (Parser)
  G.__ns_neues_element = (el) => {
    for (const [k, v] of N.attrs(el)) {
      if (!k.startsWith('on') || k.length < 3) continue;
      const typ = k.slice(2);
      try {
        const f = new Function('event', v);
        if (el.nodeName === 'BODY' && (typ === 'load' || typ === 'unload' || typ === 'resize' || typ === 'scroll')) G['on' + typ] = f;
        else {
          if (!el[HANDLER]) Object.defineProperty(el, HANDLER, { value: Object.create(null) });
          el[HANDLER][typ] = f;
          anmelden(el, typ);
        }
      } catch (e) { melden(e); }
    }
  };

  // Fensterereignisse von NetSurf (load)
  G.__ns_fenster_ereignis = (typ) => {
    if (typ === 'load') {
      bereit = 'interactive';
      const dcl = new Event('DOMContentLoaded', { bubbles: true });
      document.dispatchEvent(dcl);
      bereit = 'complete';
      document.dispatchEvent(new Event('readystatechange'));
      const ev = new Event('load');
      ev.target = document;
      fensterAufrufen(ev, 2);
    }
  };

  // ------------------------------------------------------------ Fenster
  // Timer
  let naechsteId = 1;
  const zeitgeber = new Map();
  G.__ns_zeitgeber = (id) => {
    const z = zeitgeber.get(id);
    if (!z) return;
    if (z.wiederholen) N.zeitgeber(z.ms, id); else zeitgeber.delete(id);
    try { if (typeof z.fn === 'function') z.fn(...z.args); else (0, eval)(String(z.fn)); } catch (e) { melden(e); }
  };
  function stellen(fn, ms, args, wiederholen) {
    const id = naechsteId++;
    ms = Math.max(0, +ms || 0);
    if (wiederholen) ms = Math.max(ms, 4);
    zeitgeber.set(id, { fn, ms, args, wiederholen });
    N.zeitgeber(ms, id);
    return id;
  }
  function abstellen(id) { if (zeitgeber.delete(id)) N.zeitgeberWeg(id); }
  G.setTimeout = (fn, ms, ...a) => stellen(fn, ms, a, false);
  G.setInterval = (fn, ms, ...a) => stellen(fn, ms, a, true);
  G.clearTimeout = abstellen;
  G.clearInterval = abstellen;
  G.requestAnimationFrame = (fn) => stellen(() => fn(performance.now()), 16, [], false);
  G.cancelAnimationFrame = abstellen;
  G.requestIdleCallback = (fn) => stellen(() => fn({ didTimeout: false, timeRemaining: () => 10 }), 1, [], false);
  G.cancelIdleCallback = abstellen;
  if (typeof G.queueMicrotask !== 'function') G.queueMicrotask = (fn) => { Promise.resolve().then(fn); };

  // console
  function text(x) {
    if (typeof x === 'string') return x;
    if (x instanceof Node) return `[${x.nodeName}]`;
    if (x instanceof Error) return `${x}`;
    try { return JSON.stringify(x) ?? String(x); } catch (e) { return String(x); }
  }
  const konsole = {};
  for (const art of ['log', 'info', 'warn', 'error', 'debug', 'trace']) konsole[art] = (...a) => N.protokoll(art, a.map(text).join(' '));
  konsole.assert = (b, ...a) => { if (!b) N.protokoll('assert', a.map(text).join(' ')); };
  konsole.group = konsole.groupCollapsed = konsole.groupEnd = konsole.time = konsole.timeEnd = konsole.count = () => {};
  konsole.table = (x) => N.protokoll('table', text(x));
  konsole.dir = (x) => N.protokoll('dir', text(x));
  G.console = konsole;

  // performance
  const start = Date.now();
  G.performance = { now: () => Date.now() - start, timeOrigin: start, mark() {}, measure() {}, getEntriesByType: () => [] };

  // URL und URLSearchParams (QuickJS hat keine)
  class URLSearchParams {
    constructor(init) {
      this._l = [];
      if (typeof init === 'string') {
        for (const teil of init.replace(/^\?/, '').split('&')) {
          if (!teil) continue;
          const i = teil.indexOf('=');
          const d = (t) => decodeURIComponent(t.replace(/\+/g, ' '));
          this._l.push(i < 0 ? [d(teil), ''] : [d(teil.slice(0, i)), d(teil.slice(i + 1))]);
        }
      } else if (init && typeof init === 'object') {
        for (const [k, v] of (Symbol.iterator in init ? init : Object.entries(init))) this._l.push([String(k), String(v)]);
      }
    }
    get(k) { const e = this._l.find(x => x[0] === k); return e ? e[1] : null; }
    getAll(k) { return this._l.filter(x => x[0] === k).map(x => x[1]); }
    has(k) { return this._l.some(x => x[0] === k); }
    set(k, v) { const i = this._l.findIndex(x => x[0] === k); if (i < 0) this._l.push([k, String(v)]); else { this._l[i][1] = String(v); this._l = this._l.filter((x, j) => x[0] !== k || j === i); } this._aendern(); }
    append(k, v) { this._l.push([k, String(v)]); this._aendern(); }
    delete(k) { this._l = this._l.filter(x => x[0] !== k); this._aendern(); }
    forEach(f, t) { this._l.forEach(([k, v]) => f.call(t, v, k, this)); }
    keys() { return this._l.map(x => x[0])[Symbol.iterator](); }
    values() { return this._l.map(x => x[1])[Symbol.iterator](); }
    entries() { return this._l.map(x => [x[0], x[1]])[Symbol.iterator](); }
    [Symbol.iterator]() { return this.entries(); }
    get size() { return this._l.length; }
    sort() { this._l.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0)); this._aendern(); }
    toString() { const e = (t) => encodeURIComponent(t).replace(/%20/g, '+'); return this._l.map(([k, v]) => `${e(k)}=${e(v)}`).join('&'); }
    _aendern() { if (this._url) this._url._suche = this._l.length ? '?' + this.toString() : ''; }
  }
  const URL_RE = /^([a-zA-Z][a-zA-Z0-9+.-]*):(?:\/\/(?:([^:@/?#]*)(?::([^@/?#]*))?@)?(\[[^\]]*\]|[^:/?#]*)(?::(\d*))?)?([^?#]*)(\?[^#]*)?(#.*)?$/;
  const STANDARD = { 'http:': '80', 'https:': '443', 'ftp:': '21', 'ws:': '80', 'wss:': '443' };
  function pfadAufloesen(p) {
    const aus = [];
    for (const s of p.split('/')) {
      if (s === '..') { if (aus.length > 1) aus.pop(); } else if (s !== '.') aus.push(s);
    }
    let r = aus.join('/');
    if (/\/\.\.?$/.test(p)) r += '/';
    return r.startsWith('/') ? r : '/' + r;
  }
  class URL {
    constructor(url, basis) {
      url = String(url).trim();
      let m = URL_RE.exec(url);
      if (!m) {
        if (basis === undefined) throw new TypeError(`Ungültige URL: ${url}`);
        const b = basis instanceof URL ? basis : new URL(basis);
        let ziel;
        if (url.startsWith('//')) ziel = b.protocol + url;
        else if (url.startsWith('/')) ziel = b.origin + url;
        else if (url.startsWith('?')) ziel = b.origin + b.pathname + url;
        else if (url.startsWith('#')) ziel = b.origin + b.pathname + b.search + url;
        else if (url === '') ziel = b.origin + b.pathname + b.search;
        else ziel = b.origin + b.pathname.replace(/[^/]*$/, '') + url;
        m = URL_RE.exec(ziel);
        if (!m) throw new TypeError(`Ungültige URL: ${url}`);
      }
      this._protokoll = m[1].toLowerCase() + ':';
      this._name = m[2] || '';
      this._kennwort = m[3] || '';
      this._host = (m[4] || '').toLowerCase();
      this._port = m[5] && STANDARD[this._protokoll] !== m[5] ? m[5] : '';
      this._hierarchisch = m[4] !== undefined;
      this._pfad = this._hierarchisch ? pfadAufloesen(m[6] || '/') : (m[6] || '');
      this._suche = m[7] && m[7] !== '?' ? m[7] : '';
      this._anker = m[8] && m[8] !== '#' ? m[8] : '';
    }
    get protocol() { return this._protokoll; }
    set protocol(v) { this._protokoll = String(v).replace(/:?$/, ':'); }
    get username() { return this._name; }
    get password() { return this._kennwort; }
    get hostname() { return this._host; }
    set hostname(v) { this._host = String(v).toLowerCase(); }
    get port() { return this._port; }
    set port(v) { this._port = String(v); }
    get host() { return this._host + (this._port ? ':' + this._port : ''); }
    get origin() { return this._hierarchisch ? `${this._protokoll}//${this.host}` : 'null'; }
    get pathname() { return this._pfad; }
    set pathname(v) { this._pfad = pfadAufloesen(String(v)); }
    get search() { return this._suche; }
    set search(v) { v = String(v); this._suche = v && v !== '?' ? (v.startsWith('?') ? v : '?' + v) : ''; }
    get searchParams() { const p = new URLSearchParams(this._suche); p._url = this; return p; }
    get hash() { return this._anker; }
    set hash(v) { v = String(v); this._anker = v && v !== '#' ? (v.startsWith('#') ? v : '#' + v) : ''; }
    get href() {
      const anm = this._name ? this._name + (this._kennwort ? ':' + this._kennwort : '') + '@' : '';
      return this._hierarchisch ? `${this._protokoll}//${anm}${this.host}${this._pfad}${this._suche}${this._anker}` : `${this._protokoll}${this._pfad}${this._suche}${this._anker}`;
    }
    set href(v) { Object.assign(this, new URL(v)); }
    toString() { return this.href; }
    toJSON() { return this.href; }
    static canParse(u, b) { try { new URL(u, b); return true; } catch (e) { return false; } }
  }
  G.URL = URL;
  G.URLSearchParams = URLSearchParams;

  // location
  const location = {
    get href() { return N.adresse(); },
    set href(v) { N.geheZu(String(v)); },
    assign(v) { N.geheZu(String(v)); },
    replace(v) { N.geheZu(String(v)); },
    reload() { N.geheZu(N.adresse()); },
    toString() { return N.adresse(); },
  };
  for (const k of ['protocol', 'host', 'hostname', 'port', 'pathname', 'search', 'hash', 'origin']) {
    Object.defineProperty(location, k, {
      enumerable: true,
      get() { try { return new URL(N.adresse())[k]; } catch (e) { return ''; } },
      set(v) { if (k === 'origin') return; const u = new URL(N.adresse()); u[k] = v; N.geheZu(u.href); },
    });
  }
  G.location = location;

  G.navigator = {
    userAgent: 'Mozilla/5.0 (ESP32-OS; RISC-V) NetSurf/3.11 QuickJS-ng',
    appName: 'Netscape', appVersion: '5.0', product: 'Gecko', vendor: '', platform: 'ESP32-P4',
    language: 'de-CH', languages: ['de-CH', 'de', 'en'], onLine: true, cookieEnabled: true,
    hardwareConcurrency: 2, maxTouchPoints: 2, doNotTrack: '1',
    sendBeacon() { return false; },
  };
  G.screen = { width: 1280, height: 720, availWidth: 1280, availHeight: 672, colorDepth: 16, pixelDepth: 16, orientation: { type: 'landscape-primary', angle: 0 } };
  G.innerWidth = 640; G.innerHeight = 312; G.outerWidth = 640; G.outerHeight = 360;
  G.devicePixelRatio = 2;
  G.scrollX = G.scrollY = G.pageXOffset = G.pageYOffset = 0;
  G.scrollTo = G.scroll = G.scrollBy = () => {};
  G.history = { length: 1, state: null, back() {}, forward() {}, go() {}, pushState(s) { this.state = s; }, replaceState(s) { this.state = s; } };
  G.alert = (t) => N.protokoll('alert', String(t));
  G.confirm = (t) => { N.protokoll('confirm', String(t)); return false; };
  G.prompt = (t) => { N.protokoll('prompt', String(t)); return null; };
  G.print = () => {};
  G.focus = G.blur = () => {};
  G.matchMedia = (q) => ({ matches: /max-width\s*:\s*(\d+)/.test(q) ? +RegExp.$1 >= 640 : /prefers-color-scheme\s*:\s*light/.test(q), media: q, onchange: null, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} });
  G.getComputedStyle = (el) => el.style;
  G.CSS = { escape: (s) => String(s).replace(/([^\w-])/g, '\\$1'), supports: () => false };

  // Storage (je Seite im Speicher; Ablage je Herkunft folgt)
  class Storage {
    constructor() { Object.defineProperty(this, '_m', { value: new Map() }); }
    get length() { return this._m.size; }
    key(i) { return [...this._m.keys()][i] ?? null; }
    getItem(k) { return this._m.has(String(k)) ? this._m.get(String(k)) : null; }
    setItem(k, v) { this._m.set(String(k), String(v)); }
    removeItem(k) { this._m.delete(String(k)); }
    clear() { this._m.clear(); }
  }
  G.localStorage = new Storage();
  G.sessionStorage = new Storage();

  // Base64
  const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  G.btoa = (s) => {
    s = String(s);
    let r = '';
    for (let i = 0; i < s.length; i += 3) {
      const a = s.charCodeAt(i), b = s.charCodeAt(i + 1), c = s.charCodeAt(i + 2);
      if (a > 255 || b > 255 || c > 255) throw new Error('btoa: Zeichen ausserhalb Latin-1');
      const n = (a << 16) | ((b || 0) << 8) | (c || 0);
      r += B64[n >> 18 & 63] + B64[n >> 12 & 63] + (i + 1 < s.length ? B64[n >> 6 & 63] : '=') + (i + 2 < s.length ? B64[n & 63] : '=');
    }
    return r;
  };
  G.atob = (s) => {
    s = String(s).replace(/[\s=]/g, '');
    let r = '';
    for (let i = 0; i < s.length; i += 4) {
      const n = (B64.indexOf(s[i]) << 18) | (B64.indexOf(s[i + 1]) << 12) | ((B64.indexOf(s[i + 2]) & 63) << 6) | (B64.indexOf(s[i + 3]) & 63);
      r += String.fromCharCode(n >> 16 & 255);
      if (i + 2 < s.length) r += String.fromCharCode(n >> 8 & 255);
      if (i + 3 < s.length) r += String.fromCharCode(n & 255);
    }
    return r;
  };

  // Klassen und Fensterobjekte global
  Object.assign(G, {
    EventTarget, Event, CustomEvent, UIEvent, MouseEvent, KeyboardEvent, FocusEvent, InputEvent,
    Node, CharacterData, Text, Comment, Element, HTMLElement, Document, HTMLDocument: Document, DocumentFragment,
    DocumentType, NodeList, HTMLCollection: NodeList, DOMTokenList, Storage,
    document, window: G, self: G, top: G, parent: G, frames: G, globalThis: G,
  });
  for (const t of ['Div', 'Span', 'Anchor', 'Image', 'Input', 'Button', 'Form', 'Select', 'Option', 'TextArea', 'Script', 'Style', 'Link',
    'Meta', 'Body', 'Head', 'Html', 'Paragraph', 'Heading', 'UList', 'OList', 'LI', 'Table', 'TableRow', 'TableCell', 'Label', 'IFrame',
    'Canvas', 'Video', 'Audio', 'Template', 'Pre', 'BR', 'HR', 'Title']) G[`HTML${t}Element`] = HTMLElement;
  onDefinieren(Object.getPrototypeOf(G) === Object.prototype ? G : G);
  Object.setPrototypeOf(G, EventTarget.prototype);
})();
