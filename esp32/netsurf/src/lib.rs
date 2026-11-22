//! NetSurf-Browser (Auftrag 13.9): Upstream NetSurf 3.11 mit
//! Framebuffer-Frontend, JavaScript über QuickJS-ng (c/js_quickjs.c,
//! js/vorspann.js) und Neu-Layout nach DOM-Änderungen (c/neu_layout.c).
//! Auf 32 Bit (i686-Tests, Board) ein ESP32-OS-Programm „browser“ im
//! Kern-Fenster (c/flaeche_esp32os.c, Neustart c/programm_daten.c).
//! Quellen: `werkzeuge/quellen_holen.py` (festgelegte Archive, SHA-256).

#[cfg(target_os = "espidf")]
mod ressourcen {
    include!(concat!(env!("OUT_DIR"), "/ressourcen.rs"));
}

/// Board: Ressourcen (CSS, Messages, Willkommensseiten, Symbole) als
/// schreibgeschütztes Dateisystem /nsres aus dem Firmware-Abbild anmelden
/// (firmware/komponenten/esp32os/eingebettet.c). Kein Schreiben in den
/// Flash: beim ersten Start ≈200 KB zu schreiben brachte das Board zum
/// Absturz (STATUS B1). Einmal je Systemlauf – die Marke liegt in Rust,
/// ausserhalb der statischen Daten von NetSurf, die jeder Start
/// zurückspielt.
#[cfg(target_os = "espidf")]
fn ressourcen_anmelden() -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    unsafe extern "C" {
        fn esp32os_eingebettet_anmelden(basis: *const core::ffi::c_char, dateien: *const ressourcen::Datei, n: usize) -> i32;
    }
    static ANGEMELDET: AtomicBool = AtomicBool::new(false);
    if ANGEMELDET.load(Ordering::Acquire) {
        return true;
    }
    // SAFETY: Tabelle und Namen sind statisch im Abbild.
    let r = unsafe { esp32os_eingebettet_anmelden(c"/nsres".as_ptr(), ressourcen::DATEIEN.as_ptr(), ressourcen::DATEIEN.len()) };
    if r != 0 {
        kern::sys::debug_text(&format!("browser: /nsres nicht angemeldet (Fehler {r})\n"));
        return false;
    }
    ANGEMELDET.store(true, Ordering::Release);
    true
}

/// Programm „browser“ (B1): NetSurf im Kern-Fenster (Fläche „esp32os“).
/// Startparameter: Adresse der Seite (leer: Startseite). Vor dem Start
/// werden die statischen Daten von NetSurf zurückgespielt – Programme
/// teilen den Adressraum, ein zweiter Start fände sonst den Zustand des
/// ersten vor (c/programm_daten.c).
#[cfg(target_pointer_width = "32")]
pub fn programm() {
    unsafe extern "C" {
        fn ns_programm_daten_start() -> usize;
        fn ns_esp32os_main(url: *const core::ffi::c_char) -> i32;
    }
    // NetSurf hat einen Satz statischer Daten: nur eine Instanz (auch
    // nicht, solange ein beendeter Browser noch aufgeräumt wird)
    let ich = kern::eigene_pid();
    if kern::liste().iter().any(|p| p.name == "browser" && Some(p.pid) != ich) {
        kern::sys::debug_text("browser: läuft schon\n");
        return;
    }
    #[cfg(target_os = "espidf")]
    if !ressourcen_anmelden() {
        return;
    }
    // Sprache der Meldungen (Messages unter de/ bzw. en/)
    // SAFETY: NetSurf läuft noch nicht; andere Programme lesen die
    // Umgebung nicht.
    unsafe { std::env::set_var("LANGUAGE", kern::sprache::kuerzel(kern::sprache::aktuell())) };
    // SAFETY: Aufruf im Programmfaden, einzige Instanz (oben geprüft).
    if unsafe { ns_programm_daten_start() } == 0 {
        kern::sys::debug_text("browser: statische Daten nicht gesichert (Rahmen leer oder kein Speicher)\n");
        return;
    }
    let url = std::ffi::CString::new(kern::argumente()).unwrap_or_default();
    // SAFETY: nullterminierte Adresse, lebt bis zum Ende des Aufrufs.
    unsafe {
        ns_esp32os_main(url.as_ptr());
    }
}

#[cfg(not(target_os = "espidf"))]
unsafe extern "C" {
    fn ns_probe_html(daten: *const u8, laenge: usize, tag: *const core::ffi::c_char) -> i32;
    fn ns_probe_css(daten: *const u8, laenge: usize) -> core::ffi::c_long;
    fn ns_probe_url(basis: *const core::ffi::c_char, rel: *const core::ffi::c_char, aus: *mut u8, max: usize) -> core::ffi::c_long;
    fn ns_probe_zeichensatz(richtung: i32, ein: *const u8, n: usize, enc: *const core::ffi::c_char, aus: *mut u8, max: usize) -> core::ffi::c_long;
    fn ns_probe_fb(innen: *mut u32, aussen: *mut u32, linie: *mut u32) -> i32;
    fn ns_probe_js(code: *const u8, laenge: usize, speicher: usize, max_ms: i32, aus: *mut u8, max: usize) -> i32;
    fn ns_js_konsole() -> *const core::ffi::c_char;
    fn ns_js_konsole_leeren();
    fn ns_neu_layout_anzahl() -> u32;
    fn ns_probe_setenv(name: *const core::ffi::c_char, wert: *const core::ffi::c_char) -> i32;
    fn ns_probe_lauf(
        url: *const core::ffi::c_char,
        breite: i32,
        hoehe: i32,
        ms: i32,
        js: i32,
        aus: *mut *const u8,
        b: *mut i32,
        h: *mut i32,
        zeile: *mut i32,
        updates: *mut i32,
    ) -> i32;
}

/// JavaScript (QuickJS-ng) in einer eigenen Laufzeit mit `speicher` Byte
/// Grenze und `max_ms` Zeitgrenze: Ok(JSON des Ergebnisses bzw. von
/// `globalThis.ergebnis`) oder Err(Ausnahme als Text).
#[cfg(not(target_os = "espidf"))]
pub fn js(code: &str, speicher: usize, max_ms: i32) -> Result<String, String> {
    let mut buf = vec![0u8; 4096];
    // QuickJS verlangt ein Nullbyte hinter dem Quelltext (JS_Eval)
    let Ok(c) = std::ffi::CString::new(code) else { return Err("Nullbyte im Skript".into()) };
    // SAFETY: Zeiger und Längen gültig für die Dauer des Aufrufs.
    let r = unsafe { ns_probe_js(c.as_ptr().cast(), code.len(), speicher, max_ms, buf.as_mut_ptr(), buf.len()) };
    let n = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let t = String::from_utf8_lossy(&buf[..n]).into_owned();
    match r {
        0 => Ok(t),
        1 => Err(t),
        _ => Err(format!("Laufzeit nicht angelegt ({r})")),
    }
}

/// Bild eines Prüflaufs: Breite, Höhe, Pixel 0x00RRGGBB (XRGB8888 – das
/// Framebuffer-Frontend wählt dieses Format für 32 Bit).
#[cfg(not(target_os = "espidf"))]
pub struct Bild {
    pub breite: usize,
    pub hoehe: usize,
    pub px: Vec<u32>,
    /// Anzahl Bildaktualisierungen der Fläche (> 0: es wurde gezeichnet)
    pub updates: i32,
}

/// Umgebungsvariable für die C-Seite (Windows: eigene Kopie der C-Laufzeit).
#[cfg(not(target_os = "espidf"))]
pub fn c_umgebung(name: &str, wert: &str) -> bool {
    let (n, w) = (std::ffi::CString::new(name), std::ffi::CString::new(wert));
    let (Ok(n), Ok(w)) = (n, w) else { return false };
    // SAFETY: gültige C-Zeichenketten für die Dauer des Aufrufs.
    unsafe { ns_probe_setenv(n.as_ptr(), w.as_ptr()) == 0 }
}

/// Unverändertes Framebuffer-Frontend ohne Bildschirm: `url` laden, nach
/// `ms` beenden, Bild zurück. Ressourcen über `NETSURF_RES1/2`.
#[cfg(not(target_os = "espidf"))]
pub fn lauf(url: &str, breite: i32, hoehe: i32, ms: i32) -> Option<(i32, Bild)> {
    lauf_mit(url, breite, hoehe, ms, false)
}

/// Wie [`lauf`], mit `js` = JavaScript eingeschaltet (B7).
#[cfg(not(target_os = "espidf"))]
pub fn lauf_mit(url: &str, breite: i32, hoehe: i32, ms: i32, js: bool) -> Option<(i32, Bild)> {
    let u = std::ffi::CString::new(url).ok()?;
    let (mut p, mut b, mut h, mut z, mut n) = (core::ptr::null(), 0, 0, 0, 0);
    // SAFETY: Zeiger auf lokale Variablen; das Bild bleibt bis zum nächsten
    // Lauf gültig und wird sofort kopiert.
    let r = unsafe { ns_probe_lauf(u.as_ptr(), breite, hoehe, ms, i32::from(js), &mut p, &mut b, &mut h, &mut z, &mut n) };
    if p.is_null() || b <= 0 || h <= 0 {
        return None;
    }
    let mut px = Vec::with_capacity((b * h) as usize);
    for y in 0..h as usize {
        for x in 0..b as usize {
            // SAFETY: innerhalb des kopierten Puffers (zeile × hoehe Byte).
            px.push(unsafe { core::ptr::read_unaligned(p.add(y * z as usize + x * 4).cast::<u32>()) });
        }
    }
    Some((r, Bild { breite: b as usize, hoehe: h as usize, px, updates: n }))
}

/// Ausgaben von console.* und JavaScript-Fehlern seit dem letzten Leeren
/// (`art: text` je Zeile; B7).
#[cfg(not(target_os = "espidf"))]
pub fn js_konsole(leeren: bool) -> String {
    // SAFETY: nullterminierter Puffer der C-Seite, nur aus diesem Faden.
    let t = unsafe { std::ffi::CStr::from_ptr(ns_js_konsole()) }.to_string_lossy().into_owned();
    if leeren {
        // SAFETY: setzt nur den Puffer zurück.
        unsafe { ns_js_konsole_leeren() };
    }
    t
}

/// Anzahl abgeschlossener Neu-Layouts nach DOM-Änderungen (B7).
#[cfg(not(target_os = "espidf"))]
pub fn neu_layouts() -> u32 {
    // SAFETY: liest nur einen Zähler.
    unsafe { ns_neu_layout_anzahl() }
}

/// libnsfb: auf eine RAM-Fläche zeichnen, Pixel (innen im Rechteck, aussen,
/// auf der Linie) zurücklesen; Err = Nummer der fehlgeschlagenen Prüfung.
#[cfg(not(target_os = "espidf"))]
pub fn fb_probe() -> Result<(u32, u32, u32), i32> {
    let (mut a, mut b, mut c) = (0, 0, 0);
    // SAFETY: Zeiger auf lokale Variablen.
    let r = unsafe { ns_probe_fb(&mut a, &mut b, &mut c) };
    if r == 0 { Ok((a, b, c)) } else { Err(r) }
}

/// Ergebnis einer Probe (Länge ≥ 0) in Bytes umwandeln.
#[cfg(not(target_os = "espidf"))]
fn probe_bytes(mut f: impl FnMut(*mut u8, usize) -> core::ffi::c_long) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 4096];
    let n = f(buf.as_mut_ptr(), buf.len());
    let n = usize::try_from(n).ok()?;
    buf.truncate(n);
    Some(buf)
}

/// URL mit dem NetSurf-Kern normalisieren bzw. `rel` gegen `basis` auflösen.
#[cfg(not(target_os = "espidf"))]
pub fn url(basis: &str, rel: Option<&str>) -> Option<String> {
    let b = std::ffi::CString::new(basis).ok()?;
    let r = rel.map(std::ffi::CString::new).transpose().ok()?;
    let rp = r.as_ref().map_or(core::ptr::null(), |r| r.as_ptr());
    // SAFETY: Zeiger gültig für die Dauer des Aufrufs, Puffer `max` Byte.
    let v = probe_bytes(|aus, max| unsafe { ns_probe_url(b.as_ptr(), rp, aus, max) })?;
    String::from_utf8(v).ok()
}

/// Zeichensatz wie NetSurf (`utils/utf8.c`): 0 = UTF-8 → `enc`,
/// 1 = `enc` → UTF-8, 2 = UTF-8 → HTML in `enc` (nicht Darstellbares als
/// `&#x…;`).
#[cfg(not(target_os = "espidf"))]
pub fn zeichensatz(richtung: i32, ein: &[u8], enc: &str) -> Option<Vec<u8>> {
    let e = std::ffi::CString::new(enc).ok()?;
    // SAFETY: Zeiger gültig für die Dauer des Aufrufs, Puffer `max` Byte.
    probe_bytes(|aus, max| unsafe { ns_probe_zeichensatz(richtung, ein.as_ptr(), ein.len(), e.as_ptr(), aus, max) })
}

/// HTML mit libhubbub in ein libdom-Dokument parsen; Anzahl der Elemente
/// `tag` ("*" = alle) oder None bei Fehler.
#[cfg(not(target_os = "espidf"))]
pub fn html_elemente(html: &str, tag: &str) -> Option<usize> {
    let t = std::ffi::CString::new(tag).ok()?;
    // SAFETY: Zeiger und Längen gültig für die Dauer des Aufrufs.
    let n = unsafe { ns_probe_html(html.as_ptr(), html.len(), t.as_ptr()) };
    usize::try_from(n).ok()
}

/// CSS mit libcss parsen; Grösse des Stylesheets oder der libcss-Fehler.
#[cfg(not(target_os = "espidf"))]
pub fn css_groesse(css: &str) -> Result<usize, i64> {
    // SAFETY: Zeiger und Länge gültig für die Dauer des Aufrufs.
    let n = unsafe { ns_probe_css(css.as_ptr(), css.len()) } as i64;
    if n > 0 { Ok(n as usize) } else { Err(-n - 1) }
}

#[cfg(all(test, not(target_os = "espidf")))]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// NetSurf ist einfädig gebaut (z. B. zwischengespeicherter iconv-
    /// Deskriptor in utils/utf8.c): Tests nacheinander.
    static NACHEINANDER: Mutex<()> = Mutex::new(());

    fn allein() -> std::sync::MutexGuard<'static, ()> {
        NACHEINANDER.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Führt die C-Teile eines Tests aus. 32 Bit (i686, wie am Board): als
    /// ESP32-OS-Programm im Kern – Speicher aus dem Heap des Programms,
    /// jeder Test ein neuer Programmstart mit zurückgespielten statischen
    /// Daten (Neustart, c/programm_daten.c). 64 Bit: direkt.
    #[cfg(not(target_pointer_width = "32"))]
    fn c(f: fn()) {
        let _a = allein();
        f();
    }

    #[cfg(target_pointer_width = "32")]
    fn c(f: fn()) {
        use std::time::{Duration, Instant};
        static AUFTRAG: Mutex<Option<fn()>> = Mutex::new(None);
        static ERGEBNIS: Mutex<Option<Result<usize, String>>> = Mutex::new(None);
        fn start() {
            unsafe extern "C" {
                fn ns_programm_daten_start() -> usize;
            }
            // SAFETY: einziges laufendes NetSurf-Programm (NACHEINANDER).
            let n = unsafe { ns_programm_daten_start() };
            let f = AUFTRAG.lock().unwrap().take().expect("Auftrag");
            let r = std::panic::catch_unwind(f).map(|()| n).map_err(|e| {
                e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
            });
            *ERGEBNIS.lock().unwrap() = Some(r);
        }
        static EINTRAG: kern::ProgrammEintrag = kern::ProgrammEintrag { name: "netsurf_test", heap_kb: 128 * 1024, stapel_kb: 2048, start };
        let _a = allein();
        kern::init(&kern::PC, kolibri::PROGRAMME);
        *AUFTRAG.lock().unwrap() = Some(f);
        *ERGEBNIS.lock().unwrap() = None;
        let pid = kern::starte(&EINTRAG).expect("Programm startet");
        let t = Instant::now();
        loop {
            kern::aufraeumen();
            if let Some(e) = kern::ende_von(pid) {
                let r = ERGEBNIS.lock().unwrap().take();
                match r {
                    Some(Ok(n)) => assert!(n > 0, "Abschnitt der statischen Daten leer"),
                    Some(Err(m)) => panic!("im Programm: {m}"),
                    None => panic!("Programm endete ohne Ergebnis: {e:?}"),
                }
                break;
            }
            assert!(t.elapsed() < Duration::from_secs(120), "Programm endet nicht");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn html_wird_zum_dom() {
        c(html_wird_zum_dom_innen);
    }

    fn html_wird_zum_dom_innen() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body>\
                    <h1>Überschrift</h1><p>Eins <b>fett</b></p><p>Zwei &amp; drei</p>\
                    <ul><li>a<li>b<li>c</ul></body></html>";
        assert_eq!(html_elemente(html, "p"), Some(2));
        assert_eq!(html_elemente(html, "li"), Some(3), "implizit geschlossene <li>");
        // html, head, title, body, h1, p, b, p, ul, li×3
        assert_eq!(html_elemente(html, "*"), Some(12));
        // fehlerhaftes HTML wird wie im Browser repariert
        assert_eq!(html_elemente("<p>offen<div>x", "p"), Some(1));
        assert_eq!(html_elemente("<p>offen<div>x", "html"), Some(1));
    }

    #[test]
    fn css_wird_geparst() {
        c(css_wird_geparst_innen);
    }

    fn css_wird_geparst_innen() {
        let klein = css_groesse("p { color: red }").unwrap();
        let gross = css_groesse("p { color: red } h1, h2 { font-size: 2em; margin: 0 auto } a:hover { text-decoration: underline }").unwrap();
        assert!(gross > klein, "{gross} > {klein}");
        // ungültige Deklarationen werden (wie im Browser) übersprungen
        assert!(css_groesse("p { color: ; } } {").is_ok());
    }

    /// 64 Bit (ohne Programm-Modus): NetSurf lässt sich in einem Prozess
    /// nur einmal starten (Tabellen wie `guit` bleiben gesetzt; STATUS B1
    /// „Neustart“). Jeder Test mit einem Prüflauf startet sich deshalb als
    /// eigener Kindprozess: true = im Elternprozess (Kind lief erfolgreich,
    /// Test endet), false = im Kind. 32 Bit: zurückgespielte Daten, kein
    /// Kindprozess nötig.
    fn im_eigenen_prozess(name: &str) -> bool {
        // 32 Bit: jeder Test ist ein neuer Programmstart (siehe c())
        if cfg!(target_pointer_width = "32") || std::env::var_os("NS_EINZELN").is_some() {
            return false;
        }
        let aus = std::process::Command::new(std::env::current_exe().expect("Testprogramm"))
            .args([&format!("tests::{name}"), "--exact", "--test-threads=1", "--nocapture"])
            .env("NS_EINZELN", "1")
            .output()
            .expect("Kindprozess");
        let text = format!("{}{}", String::from_utf8_lossy(&aus.stdout), String::from_utf8_lossy(&aus.stderr));
        assert!(aus.status.success() && text.contains("1 passed"), "{name} im Kindprozess:\n{text}");
        true
    }

    /// data:-URL mit vollständig prozentkodiertem Inhalt (Skripte enthalten
    /// Zeichen wie `%`, `#`, `+`, die sonst falsch ankämen).
    fn data_url(html: &str) -> String {
        let mut u = String::from("data:text/html;charset=utf-8,");
        for b in html.bytes() {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                u.push(b as char);
            } else {
                u.push_str(&format!("%{b:02X}"));
            }
        }
        u
    }

    fn ressourcen() {
        let q = std::env::var("NETSURF_QUELLEN").unwrap_or_else(|_| r"C:\ns\3.11".into());
        // Pfade ohne Laufwerk (':' trennt die Einträge), gleiches Laufwerk
        let ohne_lw = |p: &str| p.replace('\\', "/").trim_start_matches(|c: char| c.is_ascii_alphabetic()).trim_start_matches(':').to_string();
        // ohne abschliessenden Schrägstrich (stat der MSVC-Laufzeit)
        assert!(c_umgebung("NETSURF_RES1", &format!("{}/erzeugt/res", ohne_lw(&q))));
        assert!(c_umgebung("NETSURF_RES2", &format!("{}/netsurf/frontends/framebuffer/res", ohne_lw(&q))));
        assert!(c_umgebung("LANGUAGE", "de"));
    }

    /// B7: Skripte der Seite laufen in QuickJS und ändern das DOM (vor dem
    /// Layout), Ereignisse, Timer, Promise-Aufträge, console, Fehler.
    #[test]
    fn javascript_aendert_seite() {
        c(javascript_aendert_seite_innen);
    }

    fn javascript_aendert_seite_innen() {
        if im_eigenen_prozess("javascript_aendert_seite") {
            return;
        }
        ressourcen();
        js_konsole(true);
        let html = r#"<html><head><title>JS</title></head><body style="margin:0;background:white">
<div id="a" style="width:200px;height:100px;background:red"></div>
<script>
const a = document.getElementById('a');
a.style.background = 'lime';
const d = document.createElement('div');
d.setAttribute('style', 'width:100px;height:50px;background:blue');
document.body.appendChild(d);
a.classList.add('x', 'y');
a.classList.remove('y');
console.log('Klasse', a.className, document.querySelectorAll('div').length, document.getElementById('a') === a);
class K { #x = 1; get x() { return this.#x; } }
console.log('ES2022', new K().x, [1, 2, 3].at(-1), 10n ** 20n);
document.addEventListener('DOMContentLoaded', () => console.log('DCL', document.readyState));
window.addEventListener('load', () => console.log('load', document.readyState));
setTimeout((x) => console.log('timer', x), 20, 'mit Argument');
const iv = setInterval(() => { console.log('intervall'); clearInterval(iv); }, 5);
Promise.resolve().then(() => console.log('auftrag'));
a.addEventListener('click', e => console.log('click', e.target.id, e.currentTarget === a, e.eventPhase));
document.body.addEventListener('click', e => console.log('sprudelt', e.target.id, e.currentTarget.nodeName));
window.addEventListener('click', e => console.log('fenster', e.target.id, e.currentTarget === window, this === window));
a.click();
a.addEventListener('fokus', e => console.log('fokus', e.detail, e.bubbles));
document.body.addEventListener('fokus', () => console.log('FALSCH: sprudelt nicht'));
a.dispatchEvent(new CustomEvent('fokus', { detail: 7 }));
const sp = document.createElement('span');
a.appendChild(sp);
sp.addEventListener('stopp', e => { e.stopPropagation(); console.log('stopp'); });
a.addEventListener('stopp', () => console.log('FALSCH: gestoppt'));
sp.dispatchEvent(new Event('stopp', { bubbles: true }));
a.addEventListener('abbr', e => e.preventDefault());
const abbr = new Event('abbr', { cancelable: true });
console.log('abgebrochen', a.dispatchEvent(abbr), abbr.defaultPrevented);
const tb = document.createElement('tbody');
tb.innerHTML = '<tr><td>1</td><td>2</td></tr>';
console.log('tabelle', tb.querySelectorAll('td').length, tb.firstChild.nodeName);
const p = document.createElement('p');
p.innerHTML = '<b>fett</b> und <i>kursiv</i>';
console.log('innerHTML', p.innerHTML, p.textContent, p.children.length);
console.log('URL', new URL('../x?y=1#z', 'https://a.example/b/c/d').href, location.protocol);
</script>
<script>throw new Error('absichtlich');</script>
<script>console.log('weiter', typeof document.body.querySelector, document.title);</script>
</body></html>"#;
        let (r, bild) = lauf_mit(&data_url(html), 480, 320, 3000, true).expect("Bild");
        assert_eq!(r, 0, "Frontend endet normal");
        let k = js_konsole(true);
        println!("{k}");
        for z in [
            "log: Klasse x 2 true",
            "log: ES2022 1 3 ",
            "log: click a true 2",
            "log: sprudelt a BODY",
            "log: fenster a true true",
            "log: fokus 7 false",
            "log: stopp",
            "log: abgebrochen false true",
            "log: tabelle 2 TR",
            "log: innerHTML <b>fett</b> und <i>kursiv</i> fett und kursiv 2",
            "log: URL https://a.example/b/x?y=1#z data:",
            "log: auftrag",
            "absichtlich",
            "log: weiter function JS",
            "log: DCL interactive",
            "log: load complete",
            "log: intervall",
            "log: timer mit Argument",
        ] {
            assert!(k.contains(z), "fehlt in der Konsole: {z:?}\n{k}");
        }
        assert_eq!(k.matches("intervall").count(), 1, "clearInterval wirkt");
        // je Ereignis genau ein Aufruf je Hörer (libdom ruft das Ziel mehrfach)
        assert_eq!(k.matches("log: click").count(), 1, "{k}");
        assert_eq!(k.matches("log: DCL").count(), 1, "{k}");
        assert!(!k.contains("FALSCH"), "{k}");
        // Promise-Aufträge laufen nach dem Skript
        assert!(k.find("auftrag") > k.find("URL"), "{k}");
        // Bild: das Skript hat Rot durch Grün ersetzt und Blau angehängt
        let rot = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0xFF_0000).count();
        let gruen = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0x00_FF00).count();
        let blau = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0x00_00FF).count();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
        std::fs::write(dir.join("b7_pc_js_seite.png"), spatz::image::encode_png(bild.breite, bild.hoehe, &bild.px)).unwrap();
        assert_eq!(rot, 0, "kein Rot mehr");
        assert!(gruen >= 200 * 100 * 9 / 10, "grünes Rechteck: {gruen}");
        assert!(blau >= 100 * 50 * 9 / 10, "angehängtes blaues Rechteck: {blau}");
    }

    /// B7: Skripte ändern die Seite nach dem Laden (Timer): der Box-Baum
    /// wird aus dem DOM neu gebaut und neu gezeichnet – auch ein später
    /// eingefügtes Stylesheet wirkt; Formularfelder und ein iframe
    /// überstehen den Neuaufbau.
    #[test]
    fn javascript_neu_layout() {
        c(javascript_neu_layout_innen);
    }

    fn javascript_neu_layout_innen() {
        if im_eigenen_prozess("javascript_neu_layout") {
            return;
        }
        ressourcen();
        js_konsole(true);
        let html = r#"<html><head><title>Neu</title></head><body style="margin:0;background:white">
<div id="a" style="width:200px;height:100px;background:red"></div>
<form><input id="f1" value="im Formular"></form>
<input id="f2" value="frei">
<iframe id="i" src="data:text/html,%3Cbody%20style%3D%22margin:0;background:%23ff0%22%3E%3C/body%3E" style="width:120px;height:40px;border:0"></iframe>
<script>
let n = 0;
document.getElementById('a').addEventListener('klick', () => n++);
setTimeout(() => {
  const a = document.getElementById('a');
  a.style.background = 'lime';
  const d = document.createElement('div');
  d.id = 'b';
  d.style.cssText = 'width:100px;height:50px;background:blue';
  a.after(d);
  console.log('geändert', document.querySelectorAll('div').length);
}, 300);
setTimeout(() => {
  const st = document.createElement('style');
  st.textContent = '#b { width: 150px !important }';
  document.head.appendChild(st);
  document.getElementById('f2').value = 'neu';
  console.log('stil');
}, 900);
</script>
</body></html>"#;
        let vorher = neu_layouts();
        let (r, bild) = lauf_mit(&data_url(html), 480, 360, 2500, true).expect("Bild");
        assert_eq!(r, 0, "Frontend endet normal");
        let k = js_konsole(true);
        println!("{k}");
        let anzahl = neu_layouts() - vorher;
        assert!(k.contains("log: geändert 2") && k.contains("log: stil"), "{k}");
        assert!(!k.contains("Fehler"), "{k}");
        assert!(anzahl >= 2, "Neu-Layouts: {anzahl}");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
        std::fs::write(dir.join("b7_pc_neu_layout.png"), spatz::image::encode_png(bild.breite, bild.hoehe, &bild.px)).unwrap();
        let zaehlen = |farbe: u32| bild.px.iter().filter(|&&p| p & 0xFF_FFFF == farbe).count();
        let (rot, gruen, blau, gelb) = (zaehlen(0xFF_0000), zaehlen(0x00_FF00), zaehlen(0x00_00FF), zaehlen(0xFF_FF00));
        assert_eq!(rot, 0, "kein Rot mehr nach dem Timer");
        assert!(gruen >= 200 * 100 * 9 / 10, "grün: {gruen}");
        // späteres Stylesheet: Breite 150 statt 100
        assert!(blau >= 150 * 50 * 9 / 10, "blau mit Stylesheet-Breite: {blau}");
        assert!(gelb >= 100 * 30, "iframe noch da: {gelb}");
    }

    /// B1 Neustart (nur 32 Bit, wie am Board): NetSurf zweimal als
    /// Programm starten – der zweite Start läuft mit zurückgespielten
    /// statischen Daten und zeichnet dasselbe Bild.
    #[cfg(target_pointer_width = "32")]
    #[test]
    fn neustart_gleiches_bild() {
        static BILDER: Mutex<Vec<Vec<u32>>> = Mutex::new(Vec::new());
        fn einmal() {
            ressourcen();
            let html = "<html><body style='margin:0;background:white'><div style='width:120px;height:60px;background:red'></div>                        <h1 style='color:blue'>Neustart</h1></body></html>";
            let (r, bild) = lauf(&data_url(html), 320, 200, 1500).expect("Bild");
            assert_eq!(r, 0);
            BILDER.lock().unwrap().push(bild.px);
        }
        BILDER.lock().unwrap().clear();
        c(einmal);
        c(einmal);
        let b = BILDER.lock().unwrap();
        assert_eq!(b.len(), 2);
        let rot = b[1].iter().filter(|&&p| p & 0xFF_FFFF == 0xFF_0000).count();
        assert!(rot >= 120 * 60 * 9 / 10, "zweiter Start zeichnet: {rot}");
        assert!(b[0] == b[1], "beide Starts gleiches Bild");
    }

    /// B1: Programm „browser“ im Kern (i686, wie am Board): NetSurf zeichnet
    /// über die Fläche „esp32os“ in ein Kern-Fenster; nach dem Beenden
    /// startet es ein zweites Mal (zurückgespielte statische Daten).
    #[cfg(target_pointer_width = "32")]
    #[test]
    fn browser_im_kern_fenster() {
        use kern::sys;
        use std::time::{Duration, Instant};
        static BROWSER: kern::ProgrammEintrag = kern::ProgrammEintrag { name: "browser", heap_kb: 64 * 1024, stapel_kb: 2048, start: programm };
        let _a = allein();
        ressourcen();
        kern::init(&kern::PC, kolibri::PROGRAMME);
        kern::fenster::bildschirm(640, 360);
        // Startparameter sind (wie bei KolibriOS) auf 256 Zeichen begrenzt:
        // kurze Seite, nur Leerzeichen kodiert
        let html = "<div style='width:200px;height:100px;background:red'></div>                    <h1 id=t style='color:blue'>ESP32-OS</h1><script>document.getElementById('t').textContent+=' JS'</script>";
        let url = format!("data:text/html,{}", html.replace(' ', "%20"));
        assert!(url.len() <= 256, "{}", url.len());
        let rot = || sys::foto().2.iter().filter(|&&p| p & 0xFF_FFFF == 0xFF_0000).count();
        for runde in 1..=2 {
            let pid = kern::starte_mit(&BROWSER, &url).expect("browser startet");
            let t = Instant::now();
            while rot() < 200 * 100 * 9 / 10 {
                if t.elapsed() > Duration::from_secs(20) {
                    let (w, h, px) = sys::foto();
                    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
                    std::fs::write(dir.join("b1_pc_browser_fehler.ist.png"), spatz::image::encode_png(w, h, &px)).unwrap();
                    panic!("Runde {runde}: Seite erscheint nicht (rot {}), Bild b1_pc_browser_fehler.ist.png", rot());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            std::thread::sleep(Duration::from_millis(500));
            // Überschrift (blau) samt Text aus dem Skript
            let blau = sys::foto().2.iter().filter(|&&p| { let (r, g, b) = (p >> 16 & 0xFF, p >> 8 & 0xFF, p & 0xFF); b > 150 && r < 80 && g < 80 }).count();
            assert!(blau > 150, "Runde {runde}: blaue Überschrift fehlt ({blau})
{}", js_konsole(false));
            let (w, h, px) = sys::foto();
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
            std::fs::write(dir.join(format!("b1_pc_browser_{runde}.png")), spatz::image::encode_png(w, h, &px)).unwrap();
            // zweite Instanz wird abgelehnt
            if runde == 1 {
                let zweit = kern::starte_mit(&BROWSER, "about:blank").expect("startet");
                let t = Instant::now();
                while kern::ende_von(zweit).is_none() {
                    kern::aufraeumen();
                    assert!(t.elapsed() < Duration::from_secs(5), "zweite Instanz endet nicht");
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            // Schliessen-Knopf (Fenster über die ganze Fläche 640 breit; wie
            // im Test von basic_gui: x + w − 12, y = 11). Am PC kann der Kern
            // C-Programme nicht von aussen beenden (Abwickeln durch C-Rahmen).
            sys::eingabe::maus(640 - 12, 11, 1);
            std::thread::sleep(Duration::from_millis(30));
            sys::eingabe::maus(640 - 12, 11, 0);
            let t = Instant::now();
            loop {
                kern::aufraeumen();
                if kern::ende_von(pid).is_some() {
                    break;
                }
                assert!(t.elapsed() < Duration::from_secs(10), "Runde {runde}: browser endet nicht");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    #[test]
    fn zeichenflaeche_nsfb() {
        c(zeichenflaeche_nsfb_innen);
    }

    fn zeichenflaeche_nsfb_innen() {
        // XBGR8888: Rot (0xAABBGGRR = 0xFF0000FF) → Speicherwort 0x..0000FF
        let (innen, aussen, linie) = fb_probe().expect("libnsfb");
        assert_eq!(innen & 0x00FF_FFFF, 0x0000_00FF, "Rechteck rot: {innen:08x}");
        assert_eq!(aussen & 0x00FF_FFFF, 0x00FF_FFFF, "Hintergrund weiss: {aussen:08x}");
        assert_eq!(linie & 0x00FF_FFFF, 0x0000_FF00, "Linie grün: {linie:08x}");
    }

    /// B1 am PC: das unveränderte Framebuffer-Frontend rendert eine Seite
    /// (data:-URL) in die Prüf-Fläche; Bild nach tests/erwartet.
    #[test]
    fn seite_wird_gerendert() {
        c(seite_wird_gerendert_innen);
    }

    fn seite_wird_gerendert_innen() {
        if im_eigenen_prozess("seite_wird_gerendert") {
            return;
        }
        ressourcen();
        let html = "<html><head><title>ESP32-OS</title></head>\
                    <body style='margin:0;background:white'>\
                    <div style='width:200px;height:100px;background:red'></div>\
                    <h1 style='color:blue'>Hallo ESP32-OS</h1>\
                    <p>NetSurf 3.11 rendert diese Seite.</p></body></html>";
        let url = format!("data:text/html;charset=utf-8,{}", html.replace(' ', "%20").replace('#', "%23"));
        let (r, bild) = lauf(&url, 480, 320, 3000).expect("Bild");
        assert_eq!(r, 0, "Frontend endet normal");
        assert!(bild.updates > 0, "es wurde gezeichnet");
        assert_eq!((bild.breite, bild.hoehe), (480, 320));
        // Farben zählen (XRGB8888: 0x00RRGGBB)
        let rot = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0xFF_0000).count();
        let blau = bild.px.iter().filter(|&&p| { let (r, g, b) = (p >> 16 & 0xFF, p >> 8 & 0xFF, p & 0xFF); b > 150 && r < 80 && g < 80 }).count();
        // PNG zum Ansehen (0x00RRGGBB wie die Fläche)
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b1_pc_seite.png"), spatz::image::encode_png(bild.breite, bild.hoehe, &bild.px)).unwrap();
        assert!(rot >= 200 * 100 * 9 / 10, "rotes Rechteck 200×100: {rot} Pixel");
        assert!(blau > 100, "blaue Überschrift: {blau} Pixel");
    }

    /// B7: modernes JavaScript (ES2020+) mit QuickJS-ng
    #[test]
    fn javascript_modern() {
        c(javascript_modern_innen);
    }

    fn javascript_modern_innen() {
        let mb = 1 << 20;
        // ES2020: ?. ?? BigInt, Klassenfelder (ES2022), Array.at, Object.groupBy
        let r = js(
            "class Zaehler { #n = 0; plus() { return ++this.#n; } }             const z = new Zaehler(); z.plus(); z.plus();             const o = { a: { b: null } };             ({ kette: o.a?.b?.c ?? 'leer', gross: (2n ** 70n).toString(), zaehler: z.plus(),                letztes: [1, 2, 3].at(-1), gruppen: Object.keys(Object.groupBy([1, 2, 3, 4], x => x % 2 ? 'u' : 'g')).sort() })",
            8 * mb,
            1000,
        );
        assert_eq!(r.as_deref(), Ok(r#"{"kette":"leer","gross":"1180591620717411303424","zaehler":3,"letztes":3,"gruppen":["g","u"]}"#));
        // async/await + Promise-Aufträge
        let r = js("async function f() { await null; return [1, 2].map(async x => x * 2); }                    f().then(async l => { globalThis.ergebnis = await Promise.all(l); });", 8 * mb, 1000);
        assert_eq!(r.as_deref(), Ok("[2,4]"));
        // Fehler kommen als Ausnahme zurück
        assert!(js("nichtDa()", 8 * mb, 1000).unwrap_err().contains("ReferenceError"));
    }

    /// B7: Speicher- und Zeitgrenze je Seite – die Laufzeit bricht ab, der
    /// Browser (und das System) laufen weiter.
    #[test]
    fn javascript_grenzen() {
        c(javascript_grenzen_innen);
    }

    fn javascript_grenzen_innen() {
        let e = js("const a = []; while (true) a.push(new Array(1000).fill(1));", 2 << 20, 5000).unwrap_err();
        assert!(e.contains("out of memory") || e.contains("memory"), "{e}");
        let t = std::time::Instant::now();
        let e = js("for (;;) {}", 8 << 20, 200).unwrap_err();
        assert!(e.contains("interrupted"), "{e}");
        assert!(t.elapsed() < std::time::Duration::from_secs(2));
        // danach geht es normal weiter
        assert_eq!(js("6 * 7", 8 << 20, 1000).as_deref(), Ok("42"));
    }

    #[test]
    fn urls_wie_im_browser() {
        c(urls_wie_im_browser_innen);
    }

    fn urls_wie_im_browser_innen() {
        // Schema/Host klein, Standard-Port weg; Punkt-Segmente entfernt
        // NetSurf erst beim Auflösen (nsurl_join) – wie upstream
        assert_eq!(url("HTTP://Example.ORG:80/a/./b/../c?x=1#frag", None).as_deref(), Some("http://example.org/a/./b/../c?x=1#frag"));
        assert_eq!(url("http://example.org/a/b/", Some("./../c/./d")).as_deref(), Some("http://example.org/a/c/d"));
        assert_eq!(url("https://de.wikipedia.org/wiki/Hauptseite", Some("../w/index.php?search=ESP32")).as_deref(), Some("https://de.wikipedia.org/w/index.php?search=ESP32"));
        assert_eq!(url("file:///sys/netsurf/start.html", Some("bild.png")).as_deref(), Some("file:///sys/netsurf/bild.png"));
        // internationale Domain → Punycode (IDNA über utf8proc)
        assert_eq!(url("http://münchen.de/", None).as_deref(), Some("http://xn--mnchen-3ya.de/"));
    }

    #[test]
    fn zeichensaetze_ueber_parserutils() {
        c(zeichensaetze_ueber_parserutils_innen);
    }

    fn zeichensaetze_ueber_parserutils_innen() {
        // UTF-8 → ISO-8859-1 und zurück
        let latin1 = zeichensatz(0, "Grüße".as_bytes(), "ISO-8859-1").unwrap();
        assert_eq!(latin1, b"Gr\xFC\xDFe");
        assert_eq!(zeichensatz(1, &latin1, "ISO-8859-1").unwrap(), "Grüße".as_bytes());
        // Windows-1252: € = 0x80
        assert_eq!(zeichensatz(0, "5 €".as_bytes(), "Windows-1252").unwrap(), b"5 \x80");
        // ISO-8859-5 (Kyrillisch)
        assert_eq!(zeichensatz(0, "Привет".as_bytes(), "ISO-8859-5").unwrap(), b"\xBF\xE0\xD8\xD2\xD5\xE2");
        // Zeichensätze ohne Codec in libparserutils (z. B. UTF-16LE) → abgelehnt
        assert!(zeichensatz(0, b"A", "UTF-16LE").is_none());
        // nicht darstellbar: € in ISO-8859-1 → Fehler; als HTML → &#x0020ac;
        assert!(zeichensatz(0, "5 €".as_bytes(), "ISO-8859-1").is_none());
        assert_eq!(zeichensatz(2, "5 € <b>".as_bytes(), "ISO-8859-1").unwrap(), b"5 &#x0020ac; &#x00003c;b&#x00003e;");
        // unbekannter Zeichensatz
        assert!(zeichensatz(0, b"x", "KEIN-ZEICHENSATZ").is_none());
    }
}

