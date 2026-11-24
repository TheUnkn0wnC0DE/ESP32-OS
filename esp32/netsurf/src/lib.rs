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
    // Entwicklung (BR1-Lecksuche): Build mit ESP32OS_JS_AUS – Browser ohne
    // JavaScript (c/flaeche_esp32os.c liest NS_JS_AUS)
    if option_env!("ESP32OS_JS_AUS").is_some() {
        // SAFETY: wie oben, NetSurf läuft noch nicht.
        unsafe { std::env::set_var("NS_JS_AUS", "1") };
    }
    // SAFETY: Aufruf im Programmfaden, einzige Instanz (oben geprüft).
    if unsafe { ns_programm_daten_start() } == 0 {
        kern::sys::debug_text("browser: statische Daten nicht gesichert (Rahmen leer oder kein Speicher)\n");
        return;
    }
    // „--daten=<ordner> [adresse]“: eigener Datenordner für Lesezeichen,
    // Verlauf und Cookies (Board-Tests schreiben so nie in die echten
    // Daten des Nutzers; c/lesezeichen.c liest NS_DATEN_ORDNER)
    let mut argumente = kern::argumente();
    if let Some(rest) = argumente.strip_prefix("--daten=") {
        let (ordner, adresse) = rest.split_once(' ').unwrap_or((rest, ""));
        // SAFETY: wie oben, NetSurf läuft noch nicht.
        unsafe { std::env::set_var("NS_DATEN_ORDNER", ordner) };
        argumente = adresse.trim().to_string();
    }
    let url = std::ffi::CString::new(argumente).unwrap_or_default();
    // SAFETY: nullterminierte Adresse, lebt bis zum Ende des Aufrufs.
    unsafe {
        ns_esp32os_main(url.as_ptr());
    }
}

/// B6: Textfeld-Fokus aus der fbtk-Kopie (jede Schreibmarke): am Board und
/// im 32-Bit-Programm an den Kern (Bildschirmtastatur erscheint und
/// verdeckt das Feld nicht), am PC (64 Bit) für die Tests festgehalten.
#[unsafe(no_mangle)]
pub extern "C" fn ns_tastatur_fokus(an: i32, x: i32, y: i32, hoehe: i32) {
    #[cfg(target_pointer_width = "32")]
    {
        kern::sys::textfeld_fokus(an != 0);
        if an != 0 {
            kern::sys::textfeld_lage(x, y, 2, hoehe.max(1));
        }
    }
    TASTATUR.lock().unwrap_or_else(|e| e.into_inner()).push((an != 0, x, y, hoehe));
}

/// Fokus-Meldungen an die Bildschirmtastatur seit dem letzten Abholen
/// (an, x, y, Höhe) – für die Tests.
pub fn tastatur_meldungen() -> Vec<(bool, i32, i32, i32)> {
    std::mem::take(&mut *TASTATUR.lock().unwrap_or_else(|e| e.into_inner()))
}

static TASTATUR: std::sync::Mutex<Vec<(bool, i32, i32, i32)>> = std::sync::Mutex::new(Vec::new());

/// B8 (c/flaeche_esp32os.c): Lage des letzten Drückens einer Maustaste
/// (Arbeitsbereich), damit Markieren dort beginnt, wo der Finger aufsetzte.
#[cfg(target_pointer_width = "32")]
#[unsafe(no_mangle)]
pub extern "C" fn ns_druck_lage_holen(x: *mut i32, y: *mut i32) -> bool {
    match kern::sys::druck_lage_holen() {
        Some((dx, dy)) => {
            // SAFETY: Zeiger vom Aufrufer (lokale Variablen).
            unsafe {
                *x = dx;
                *y = dy;
            }
            true
        }
        None => false,
    }
}

/// B8 (c/flaeche_esp32os.c): Zeichen der Bildschirmtastatur in cp866 als
/// Unicode (Umlaute u. a.).
#[cfg(target_pointer_width = "32")]
#[unsafe(no_mangle)]
pub extern "C" fn ns_cp866_zu_uni(c: u8) -> u32 {
    kern::schrift::cp866_zu_uni(c)
}

/// B5: Zoom-Geste des Desktops (Faktor in Tausendstel seit dem letzten
/// Abholen, Mittelpunkt) für die Fläche des Browsers (c/flaeche_esp32os.c).
#[cfg(target_pointer_width = "32")]
#[unsafe(no_mangle)]
pub extern "C" fn ns_zoom_holen(faktor: *mut u32, x: *mut i32, y: *mut i32) {
    let (f, mx, my) = kern::sys::zoom_holen();
    // SAFETY: Zeiger vom Aufrufer (lokale Variablen).
    unsafe {
        *faktor = f;
        *x = mx;
        *y = my;
    }
}

/// Board, Messung B5 (c/flaeche_esp32os.c, Zeile „Wischen“): Zeichenaufrufe
/// des Kerns – Warten auf die Sperre und Arbeit (µs), Anzahl.
#[cfg(target_os = "espidf")]
#[unsafe(no_mangle)]
pub extern "C" fn ns_kern_zeichnen_messung(warten: *mut u32, arbeit: *mut u32, aufrufe: *mut u32) {
    let (w, a, n) = kern::sys::zeichnen_messung_nehmen();
    // SAFETY: Zeiger vom Aufrufer (lokale Variablen).
    unsafe {
        *warten = w;
        *arbeit = a;
        *aufrufe = n;
    }
}

/// Board, Messung (c/flaeche_esp32os.c, Zeile „ns_takt“): belegter Heap
/// des Browsers und Höchststand in KB.
#[cfg(target_os = "espidf")]
#[unsafe(no_mangle)]
pub extern "C" fn ns_heap_stand_kb(belegt: *mut u32, spitze: *mut u32) {
    let (b, s) = kern::prozess::liste()
        .iter()
        .find(|p| p.name == "browser")
        .map_or((0, 0), |p| ((p.heap_belegt / 1024) as u32, (p.heap_spitze / 1024) as u32));
    // SAFETY: Zeiger vom Aufrufer (lokale Variablen).
    unsafe {
        *belegt = b;
        *spitze = s;
    }
}

/// B8 (c/ablage.c): Zwischenablage. Am Board und im 32-Bit-Programm die des
/// Systems (Funktion 54), am PC (64 Bit) ein Speicher für die Tests.
#[cfg(not(target_pointer_width = "32"))]
static ABLAGE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Text in die Zwischenablage (aus NetSurfs Kopieren).
///
/// # Safety
/// `text` zeigt auf `laenge` lesbare Bytes (UTF-8).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ablage_setzen(text: *const u8, laenge: usize) {
    if text.is_null() {
        return;
    }
    // SAFETY: laut Aufrufer.
    let t = String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(text, laenge) }).into_owned();
    #[cfg(target_pointer_width = "32")]
    {
        kern::sys::ablage_text_setzen(&t);
        // Nachweis im Log: Anfang des kopierten Textes (Text einer Webseite)
        let anfang: String = t.chars().take(60).collect();
        kern::sys::debug_text(&format!("browser: kopiert ({} Zeichen): {anfang}\n", t.chars().count()));
    }
    #[cfg(not(target_pointer_width = "32"))]
    {
        *ABLAGE.lock().unwrap_or_else(|e| e.into_inner()) = t;
    }
    ABLAGE_ZAEHLER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

static ABLAGE_ZAEHLER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Wie oft der Browser schon etwas in die Zwischenablage gelegt hat (die
/// gui.c-Kopie meldet „Text kopiert“ nur, wenn es mehr geworden sind).
#[unsafe(no_mangle)]
pub extern "C" fn ns_ablage_zaehler() -> u32 {
    ABLAGE_ZAEHLER.load(std::sync::atomic::Ordering::Relaxed)
}

/// Inhalt der Zwischenablage (Text) – auch für die Tests.
pub fn ablage_inhalt() -> String {
    #[cfg(target_pointer_width = "32")]
    {
        kern::sys::ablage_text().unwrap_or_default()
    }
    #[cfg(not(target_pointer_width = "32"))]
    {
        ABLAGE.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Länge des Textes in der Zwischenablage in Byte.
#[unsafe(no_mangle)]
pub extern "C" fn ns_ablage_laenge() -> usize {
    ablage_inhalt().len()
}

/// Text der Zwischenablage nach `ziel` kopieren (höchstens `max` Byte, ohne
/// Null am Ende); Rückgabe: kopierte Bytes.
///
/// # Safety
/// `ziel` zeigt auf `max` beschreibbare Bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ablage_kopieren(ziel: *mut u8, max: usize) -> usize {
    if ziel.is_null() {
        return 0;
    }
    let t = ablage_inhalt();
    let n = t.len().min(max);
    // SAFETY: laut Aufrufer.
    unsafe { core::ptr::copy_nonoverlapping(t.as_ptr(), ziel, n) };
    n
}

/// B8 (c/bild.c): Breite und Höhe eines JPEG/PNG aus dem Dateikopf.
///
/// # Safety
/// `daten` zeigt auf `laenge` lesbare Bytes, `breite`/`hoehe` sind gültig.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bild_masse(daten: *const u8, laenge: usize, breite: *mut u32, hoehe: *mut u32) -> bool {
    if daten.is_null() || laenge == 0 {
        return false;
    }
    // SAFETY: laut Aufrufer.
    let d = unsafe { core::slice::from_raw_parts(daten, laenge) };
    match kolibri::dekoder::masse(d) {
        Some((w, h)) => {
            // SAFETY: laut Aufrufer.
            unsafe {
                *breite = w;
                *hoehe = h;
            }
            true
        }
        None => false,
    }
}

/// Der Dekoder (Rust) belegt im Faden des Browsers – also im Heap des
/// Browsers, wie NetSurf selbst. Ist der voll, beendet der Kern das Programm
/// („Speicher voll“, B9: tagesschau.de). Deshalb vorher prüfen, ob das Bild
/// sicher hineinpasst – sonst bleibt es ein leerer Kasten. Bedarf je
/// Bildpunkt: Ausgabe 4 Byte, progressives JPEG bis 6 Byte Koeffizienten +
/// 3 Byte Ebenen, PNG ~5 Byte entpackt → 16 Byte, dazu die Reserve des Kerns
/// und 256 KB für den Rest der Seite.
/// Dazu muss die grösste einzelne Belegung des Dekoders am Stück passen
/// (O1, Board: ein progressives JPEG beendete den Browser bei zerstückeltem
/// Heap, obwohl insgesamt genug frei war): Ausgabe 4 Byte, entpackte
/// PNG-Zeilen ~5 Byte je Bildpunkt → 6 Byte, dazu 64 KB.
#[cfg(target_pointer_width = "32")]
fn bild_passt(pixel: u64) -> bool {
    let gesamt = kern::sys::programm_speicher_frei().is_none_or(|frei| frei as u64 >= pixel * 16 + 512 * 1024);
    let am_stueck = kern::sys::programm_speicher_groesster_block().is_none_or(|g| g as u64 >= pixel * 6 + 64 * 1024);
    gesamt && am_stueck
}

#[cfg(not(target_pointer_width = "32"))]
fn bild_passt(_pixel: u64) -> bool {
    true
}

/// B8 (c/bild.c): Bild dekodieren und in die Bitmap schreiben (nicht
/// vormultipliziertes RGBA, Byte-Lage `r`/`g`/`b`/`a` je Bildpunkt), um
/// `teiler` verkleinert (Mittelwert je Block). Rückgabe 1 = deckend,
/// 0 = mit Transparenz, < 0 = Fehler (-1 Format, -2 Masse, -3 Speicher).
///
/// # Safety
/// `daten` zeigt auf `laenge` lesbare Bytes, `ziel` auf `zeile * hoehe`
/// beschreibbare Bytes (`zeile` ≥ 4 · `breite`).
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_bild_dekodieren(
    daten: *const u8,
    laenge: usize,
    ziel: *mut u8,
    breite: u32,
    hoehe: u32,
    zeile: usize,
    teiler: u32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) -> i32 {
    if daten.is_null() || ziel.is_null() || zeile < breite as usize * 4 {
        return -2;
    }
    // SAFETY: laut Aufrufer.
    let d = unsafe { core::slice::from_raw_parts(daten, laenge) };
    let Some((w, h)) = kolibri::dekoder::masse(d) else { return -1 };
    if !bild_passt(u64::from(w) * u64::from(h)) {
        return -3;
    }
    let bild = match kolibri::dekoder::dekodieren(d) {
        Ok((_, mut v)) if !v.is_empty() => v.swap_remove(0),
        _ => return -1,
    };
    let t = teiler.max(1);
    if bild.w.div_ceil(t) != breite || bild.h.div_ceil(t) != hoehe || bild.px.len() < (bild.w * bild.h) as usize {
        return -2;
    }
    // SAFETY: laut Aufrufer.
    let aus = unsafe { core::slice::from_raw_parts_mut(ziel, zeile * hoehe as usize) };
    let mut deckend = true;
    for y in 0..hoehe {
        let zeile_aus = &mut aus[y as usize * zeile..][..breite as usize * 4];
        for x in 0..breite {
            let (mut s, mut n) = ([0u32; 4], 0u32);
            for yy in y * t..((y + 1) * t).min(bild.h) {
                for xx in x * t..((x + 1) * t).min(bild.w) {
                    let p = bild.px[(yy * bild.w + xx) as usize];
                    s[0] += (p >> 16) & 0xFF;
                    s[1] += (p >> 8) & 0xFF;
                    s[2] += p & 0xFF;
                    s[3] += p >> 24;
                    n += 1;
                }
            }
            let n = n.max(1);
            let px = &mut zeile_aus[x as usize * 4..][..4];
            px[usize::from(r)] = (s[0] / n) as u8;
            px[usize::from(g)] = (s[1] / n) as u8;
            px[usize::from(b)] = (s[2] / n) as u8;
            let alpha = (s[3] / n) as u8;
            px[usize::from(a)] = alpha;
            deckend &= alpha == 0xFF;
        }
    }
    i32::from(deckend)
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
    fn ns_ohne_js_rueckfaelle() -> u32;
    fn ns_llcache_stand(groesse: *mut u32, grenze: *mut u32, frei: *mut u32);
    fn ns_ohne_js_test_setzen(adresse: *const core::ffi::c_char) -> bool;
    fn ns_eingabe_art(text: *const core::ffi::c_char, aus: *mut u8, max: usize) -> i32;
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

/// Regel der Adressleiste (B2, c/eingabe.c): 0 = Suche, 1 = Adresse wie
/// eingegeben, 2 = Adresse mit https:// davor; dazu der bereinigte Text.
#[cfg(not(target_os = "espidf"))]
pub fn eingabe_art(text: &str) -> (i32, String) {
    let t = std::ffi::CString::new(text).unwrap_or_default();
    let mut aus = vec![0u8; 512];
    // SAFETY: nullterminierter Text, Puffer mit Länge.
    let art = unsafe { ns_eingabe_art(t.as_ptr(), aus.as_mut_ptr(), aus.len()) };
    let n = aus.iter().position(|&b| b == 0).unwrap_or(aus.len());
    (art, String::from_utf8_lossy(&aus[..n]).into_owned())
}

/// Adresse auf die Liste „ohne JavaScript“ setzen (c/ohne_js.c, Tests).
#[cfg(not(target_os = "espidf"))]
pub fn ohne_js_setzen(adresse: &str) -> bool {
    let a = std::ffi::CString::new(adresse).unwrap_or_default();
    // SAFETY: nullterminierte Adresse.
    unsafe { ns_ohne_js_test_setzen(a.as_ptr()) }
}

/// Zwischenspeicher nach dem letzten Aufräumen (B3, llcache-Kopie):
/// (Gesamtgrösse, Grenze, freigebbarer Teil) in Bytes.
#[cfg(not(target_os = "espidf"))]
pub fn zwischenspeicher() -> (u32, u32, u32) {
    let (mut g, mut l, mut f) = (0u32, 0u32, 0u32);
    // SAFETY: drei lokale Zahlen.
    unsafe { ns_llcache_stand(&mut g, &mut l, &mut f) };
    (g, l, f)
}

/// Anzahl automatischer Rückfälle „ohne JavaScript“ (c/ohne_js.c).
#[cfg(not(target_os = "espidf"))]
pub fn ohne_js_rueckfaelle() -> u32 {
    // SAFETY: liest nur einen Zähler.
    unsafe { ns_ohne_js_rueckfaelle() }
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
            .args([&format!("tests::{name}"), "--exact", "--test-threads=1", "--nocapture", "--include-ignored"])
            .env("NS_EINZELN", "1")
            .output()
            .expect("Kindprozess");
        let text = format!("{}{}", String::from_utf8_lossy(&aus.stdout), String::from_utf8_lossy(&aus.stderr));
        assert!(aus.status.success() && text.contains("1 passed"), "{name} im Kindprozess:\n{text}");
        // Ausgaben des Kindes (nur sichtbar mit --nocapture)
        print!("{text}");
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
        // eigene Ressourcen (Startseite, Suchanbieter)
        let res = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("res");
        assert!(c_umgebung("NETSURF_RES3", &ohne_lw(&res.to_string_lossy())));
    }

    /// B7: Skripte der Seite laufen in QuickJS und ändern das DOM (vor dem
    /// Layout), Ereignisse, Timer, Promise-Aufträge, console, Fehler.
    /// Kleiner HTTP-Prüfserver: Pfad → (Kopfzeilen, Inhalt); POST-Inhalt
    /// kommt bei „/post“ als Antwort zurück.
    fn pruefserver(antworten: fn(&str) -> (u16, &'static str, String)) -> u16 {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").expect("Prüfserver");
        let port = server.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in server.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut s = s;
                    let mut kopf = Vec::new();
                    let mut b = [0u8; 1];
                    while !kopf.ends_with(b"\r\n\r\n") && s.read(&mut b).map(|n| n == 1).unwrap_or(false) {
                        kopf.push(b[0]);
                    }
                    let k = String::from_utf8_lossy(&kopf).to_string();
                    let pfad = k.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let laenge = k.lines().find_map(|z| z.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                    let mut rumpf = vec![0u8; laenge];
                    let _ = s.read_exact(&mut rumpf);
                    let (status, zusatz, inhalt) = if pfad == "/post" {
                        (200, "Content-Type: text/plain\r\n", format!("erhalten:{}", String::from_utf8_lossy(&rumpf)))
                    } else if pfad == "/cookie" {
                        // Cookies der Anfrage zurück
                        let c = k.lines().find_map(|z| z.strip_prefix("Cookie:").or_else(|| z.strip_prefix("cookie:"))).unwrap_or("").trim().to_string();
                        (200, "Content-Type: text/plain\r\n", format!("cookies:{c}"))
                    } else if pfad == "/setzen" {
                        (200, "Content-Type: text/plain\r\nSet-Cookie: srv=2; Path=/\r\n", "gesetzt".into())
                    } else {
                        antworten(&pfad)
                    };
                    let _ = s.write_all(format!("HTTP/1.0 {status} X\r\n{zusatz}Content-Length: {}\r\n\r\n{inhalt}", inhalt.len()).as_bytes());
                });
            }
        });
        port
    }

    /// B7: fetch und XMLHttpRequest über NetSurfs Abruf – JSON, Status 404,
    /// POST mit Inhalt, XHR mit responseType „json“, fremde Herkunft nur mit
    /// Access-Control-Allow-Origin; das Ergebnis ändert die Seite.
    #[test]
    fn javascript_fetch_xhr() {
        c(javascript_fetch_xhr_innen);
    }

    fn javascript_fetch_xhr_innen() {
        if im_eigenen_prozess("javascript_fetch_xhr") {
            return;
        }
        ressourcen();
        js_konsole(true);
        // fremde Herkunft (anderer Port)
        let fremd = pruefserver(|pfad| match pfad {
            "/offen" => (200, "Content-Type: text/plain\r\nAccess-Control-Allow-Origin: *\r\n", "frei".into()),
            _ => (200, "Content-Type: text/plain\r\n", "geheim".into()),
        });
        FREMD.store(fremd, std::sync::atomic::Ordering::Relaxed);
        static FREMD: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
        let port = pruefserver(|pfad| match pfad {
            "/daten.json" => (200, "Content-Type: application/json\r\n", r#"{"wert":42,"text":"Grüße"}"#.into()),
            "/langsam.png" => {
                // DOMContentLoaded wartet nicht auf das Bild
                std::thread::sleep(std::time::Duration::from_millis(1500));
                (200, "Content-Type: image/png\r\n", "x".into())
            }
            "/" => {
                let f = FREMD.load(std::sync::atomic::Ordering::Relaxed);
                (
                    200,
                    "Content-Type: text/html; charset=utf-8\r\n",
                    format!(
                        r#"<html><body style="margin:0;background:white"><div id="z" style="width:200px;height:100px;background:red">x</div>
<img src="/langsam.png" width="10" height="10">
<script>
let dclZeit = -1;
document.addEventListener('DOMContentLoaded', () => {{ dclZeit = performance.now(); console.log('dcl frueh', dclZeit < 1000, document.readyState); }});
window.addEventListener('load', () => console.log('load nach dcl', dclZeit >= 0, document.readyState));
fetch('/daten.json').then(r => {{ console.log('fetch', r.status, r.ok, r.headers.get('content-type')); return r.json(); }})
  .then(j => {{ console.log('json', j.wert, j.text); const z = document.getElementById('z'); z.textContent = j.wert; z.style.background = 'lime'; }});
fetch('/fehlt').then(r => console.log('404', r.status, r.ok));
fetch('/post', {{ method: 'POST', body: 'a=1&b=2' }}).then(r => r.text()).then(t => console.log('post', t));
const x = new XMLHttpRequest();
x.responseType = 'json';
x.onload = () => console.log('xhr', x.status, x.response.wert, x.readyState);
x.open('GET', '/daten.json');
x.send();
fetch('http://127.0.0.1:{f}/offen').then(r => r.text()).then(t => console.log('cors', t));
fetch('http://127.0.0.1:{f}/zu').then(r => r.text()).then(t => console.log('FALSCH: gelesen', t)).catch(e => console.log('cors blockiert', e instanceof TypeError));
try {{ fetch('/x', {{ method: 'PUT' }}).catch(e => console.log('methode', e.message)); }} catch (e) {{ console.log('methode sofort', e.message); }}
document.cookie = 'js=1; path=/';
fetch('/setzen').then(r => r.text()).then(() => {{
  console.log('doc.cookie', document.cookie.split('; ').sort().join(','));
  return fetch('/cookie');
}}).then(r => r.text()).then(t => console.log('server', t.replace('cookies:', '').split(/;\s*/).sort().join(',')));
</script></body></html>"#
                    ),
                )
            }
            _ => (404, "Content-Type: text/plain\r\n", "fehlt".into()),
        });
        let (r, bild) = lauf_mit(&format!("http://127.0.0.1:{port}/"), 480, 320, 4000, true).expect("Bild");
        assert_eq!(r, 0, "Frontend endet normal");
        let k = js_konsole(true);
        println!("{k}");
        for z in [
            "log: fetch 200 true application/json",
            "log: json 42 Grüße",
            "log: 404 404 false",
            "log: post erhalten:a=1&b=2",
            "log: xhr 200 42 4",
            "log: cors frei",
            "log: cors blockiert true",
            "log: methode fetch: Methode PUT nicht unterstützt (GET, HEAD, POST)",
            // Cookies: von JS gesetzt geht an den Server, vom Server gesetzt
            // sieht JS
            "log: doc.cookie js=1,srv=2",
            // DOMContentLoaded gleich nach dem Parsen – nicht erst nach dem
            // Bild, das 1,5 s braucht; load danach
            "log: dcl frueh true interactive",
            "log: load nach dcl true complete",
            "log: server js=1,srv=2",
        ] {
            assert!(k.contains(z), "fehlt in der Konsole: {z}");
        }
        assert!(!k.contains("FALSCH"), "{k}");
        // das Ergebnis des Abrufs hat die Seite geändert (grün)
        let px = bild.px[50 * bild.breite as usize + 100] & 0xFF_FFFF;
        assert_eq!(px, 0x00_FF00, "Block nach fetch grün");
    }

    // Cookie-Fix in urldb.c: Test nur lokal (build.rs, urldb_kopie)
    #[cfg(ns_cookie_fix)]
    include!("cookie_fix_test.local.rs");

    /// B7: Masse aus dem Layout – getBoundingClientRect, offset*/client*,
    /// getComputedStyle, display:none und entfernte Knoten (0, kein Absturz),
    /// scrollTo/scrollIntoView, normalize.
    #[test]
    fn javascript_masse() {
        c(javascript_masse_innen);
    }

    fn javascript_masse_innen() {
        if im_eigenen_prozess("javascript_masse") {
            return;
        }
        ressourcen();
        js_konsole(true);
        let html = r#"<html><head><style>body{margin:0} #a{width:200px;height:100px;padding:10px;border:5px solid black;background:#ff0000;color:#0000ff;font-size:20px}</style></head>
<body><div id="a">x</div><div id="lang" style="height:3000px"></div><div id="b" style="height:20px">b</div><div style="height:1000px"></div><span id="w" style="display:none">w</span>
<script>
setTimeout(() => {
  const a = document.getElementById('a');
  const r = a.getBoundingClientRect();
  console.log('rechteck', r.x, r.y, r.width, r.height, r.right, r.bottom);
  console.log('offset', a.offsetWidth, a.offsetHeight, a.offsetLeft, a.offsetTop, 'client', a.clientWidth, a.clientHeight);
  const s = getComputedStyle(a);
  console.log('stil', s.display, s.width, s.height, s.color, s.backgroundColor, s.fontSize, s.getPropertyValue('position'));
  console.log('versteckt', document.getElementById('w').getBoundingClientRect().width, getComputedStyle(document.getElementById('w')).display);
  const weg = document.getElementById('lang');
  weg.remove();
  console.log('entfernt', weg.getBoundingClientRect().width, weg.offsetHeight);
  document.body.insertBefore(weg, document.getElementById('b'));
  console.log('fenster', innerWidth > 0, innerHeight > 0, scrollY);
  scrollTo(0, 500);
  console.log('gerollt', scrollY, document.documentElement.scrollTop);
  document.getElementById('b').scrollIntoView();
  console.log('ins bild', scrollY > 2500, Math.round(document.getElementById('b').getBoundingClientRect().top));
  const p = document.createElement('p');
  p.appendChild(document.createTextNode('a'));
  p.appendChild(document.createTextNode(''));
  p.appendChild(document.createTextNode('b'));
  p.normalize();
  console.log('normalize', p.childNodes.length, p.textContent);
}, 500);
</script></body></html>"#;
        let (r, _bild) = lauf_mit(&data_url(html), 480, 320, 2500, true).expect("Bild");
        assert_eq!(r, 0, "Frontend endet normal");
        let k = js_konsole(true);
        println!("{k}");
        for z in [
            "log: rechteck 0 0 230 130 230 130",
            "log: offset 230 130 0 0 client 220 120",
            "log: stil block 200px 100px rgb(0, 0, 255) rgb(255, 0, 0) 20px static",
            "log: versteckt 0 none",
            "log: entfernt 0 0",
            "log: fenster true true 0",
            "log: gerollt 500 500",
            "log: ins bild true 0",
            "log: normalize 1 ab",
        ] {
            assert!(k.contains(z), "fehlt in der Konsole: {z}");
        }
    }

    /// B7: Formulare mit Skript, per Tippen und Antippen bedient – „input“
    /// je Zeichen, „submit“ vor dem Abschicken (abbrechbar), value/focus()
    /// setzen NetSurfs Feld (Tippen ohne Antippen landet dort), select()
    /// lässt das nächste Zeichen alles ersetzen, abgebrochener Klick folgt
    /// dem Link nicht, checked/reset() ziehen NetSurfs Kästchen nach,
    /// submit() schickt ohne Ereignis ab. Das Neu-Layout nach der Eingabe
    /// zeigt die neue Höhe.
    #[test]
    fn javascript_formular() {
        c(javascript_formular_innen);
    }

    fn javascript_formular_innen() {
        if im_eigenen_prozess("javascript_formular") {
            return;
        }
        ressourcen();
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_TEST_EINGABE", "");
            }
        }
        let _z = Zuruecksetzen;
        js_konsole(true);
        static ANFRAGEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        ANFRAGEN.lock().unwrap().clear();
        let port = pruefserver(|pfad| {
            ANFRAGEN.lock().unwrap().push(pfad.to_string());
            if pfad != "/" {
                return (200, "Content-Type: text/html\r\n", "<html><body style='background:#0000ff'>Ziel</body></html>".into());
            }
            (
                200,
                "Content-Type: text/html; charset=utf-8\r\n",
                r#"<html><body style="margin:0;background:white">
<form id="f" action="/ziel" method="get">
<input id="q" name="q" style="display:block;margin:0;border:0;padding:0;width:300px;height:40px;font-size:24px;background:#eee">
<input id="k" type="checkbox" name="k" style="display:block;margin:0;width:40px;height:40px">
<textarea id="t" name="t" style="display:block;margin:0;border:0;padding:0;width:300px;height:60px">vorgabe</textarea>
</form>
<a id="l" href="/weg" style="display:block;width:200px;height:40px;background:#ccc">link</a>
<div id="aus"></div>
<script>
const q = document.getElementById('q'), f = document.getElementById('f'), k = document.getElementById('k');
const t = document.getElementById('t'), aus = document.getElementById('aus');
let n = 0, runde = 0;
q.addEventListener('input', () => n++);
f.addEventListener('submit', (e) => {
  runde++;
  console.log('submit', runde, q.value, n, document.activeElement === q);
  e.preventDefault();
  if (runde === 1) { aus.style.cssText = 'height:50px;background:#00ff00'; q.select(); }
  if (runde === 2) setTimeout(() => {
    console.log('aus hoehe', aus.offsetHeight);
    f.reset();
    console.log('reset', JSON.stringify([q.value, t.value, k.checked]));
  }, 200);
});
document.getElementById('l').addEventListener('click', (e) => { e.preventDefault(); console.log('klick verhindert', location.pathname); });
k.addEventListener('click', () => console.log('kaestchen klick', k.checked));
k.addEventListener('change', () => console.log('kaestchen change', k.checked));
setTimeout(() => {
  q.value = 'vor'; q.focus();
  console.log('fokus', document.activeElement === q, q.value, q.defaultValue === '', t.value, t.defaultValue);
  t.value = 'neu'; console.log('textarea', t.value, t.defaultValue);
  k.checked = true; console.log('kaestchen', k.checked, k.defaultChecked);
}, 300);
setTimeout(() => f.submit(), 3000);
</script></body></html>"#
                    .into(),
            )
        });
        // Seite ab y 30 (Werkzeugleiste): q 30–70, k 70–110, t ab 110
        // (NetSurf nimmt die Höhe aus rows: 37), Link 147–187. Tippen ohne Antippen (Fokus vom Skript), Eingabe,
        // „z“ ersetzt die Auswahl, Link antippen, Kästchen antippen.
        assert!(c_umgebung("NS_TEST_EINGABE", "1000:T:abc\\n;1600:T:z\\n;2200:100:167;2600:20:90"));
        let (r, _bild) = lauf_mit(&format!("http://127.0.0.1:{port}/"), 800, 480, 4500, true).expect("Bild");
        assert_eq!(r, 0, "Frontend endet normal");
        let k = js_konsole(true);
        let a = ANFRAGEN.lock().unwrap().clone();
        println!("{k}\nAnfragen {a:?}");
        for z in [
            "log: fokus true vor true vorgabe vorgabe",
            "log: textarea neu vorgabe",
            "log: kaestchen true false",
            "log: submit 1 vorabc 3 true",
            "log: submit 2 z 4 true",
            "log: aus hoehe 50",
            r#"log: reset ["","vorgabe",false]"#,
            "log: klick verhindert /",
            "log: kaestchen klick true",
            "log: kaestchen change true",
        ] {
            assert!(k.contains(z), "fehlt in der Konsole: {z}");
        }
        assert!(!k.contains("submit 3"), "submit() ohne Ereignis");
        assert!(!a.iter().any(|p| p == "/weg"), "abgebrochener Klick folgt dem Link nicht: {a:?}");
        assert!(a.iter().any(|p| p == "/ziel?q=&k=on&t=vorgabe"), "submit() schickt den zurückgesetzten Stand ab: {a:?}");
    }

    /// B7: Hilfen der Fensterumgebung – console.group/count, performance-
    /// Marken und -Messungen, matchMedia mit Hörer, XHR mit
    /// overrideMimeType (Bytes einzeln), document.cookie nur mit bekannten
    /// Merkmalen, fremde Herkunft mit Cookies nur mit genauer Freigabe und
    /// Allow-Credentials, AbortController stoppt einen langsamen Abruf,
    /// defaultValue, history.length und history.back() über NetSurfs
    /// Verlauf (Seite a → b → zurück auf a).
    #[test]
    fn javascript_fenster_hilfen() {
        c(javascript_fenster_hilfen_innen);
    }

    fn javascript_fenster_hilfen_innen() {
        if im_eigenen_prozess("javascript_fenster_hilfen") {
            return;
        }
        ressourcen();
        js_konsole(true);
        static BESUCHE_A: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        BESUCHE_A.store(0, std::sync::atomic::Ordering::Relaxed);
        static HAUPT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
        static FREMD: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
        // fremde Herkunft (anderer Port, gleicher Rechner: bekommt die Cookies mit)
        let fremd = pruefserver(|pfad| {
            let herkunft = format!("http://127.0.0.1:{}", HAUPT.load(std::sync::atomic::Ordering::Relaxed));
            match pfad {
                "/stern" => (200, "Content-Type: text/plain\r\nAccess-Control-Allow-Origin: *\r\n", "x".into()),
                "/genau" => (200, Box::leak(format!("Content-Type: text/plain\r\nAccess-Control-Allow-Origin: {herkunft}\r\nAccess-Control-Allow-Credentials: true\r\n").into_boxed_str()), "x".into()),
                _ => (200, Box::leak(format!("Content-Type: text/plain\r\nAccess-Control-Allow-Origin: {herkunft}\r\n").into_boxed_str()), "x".into()),
            }
        });
        FREMD.store(fremd, std::sync::atomic::Ordering::Relaxed);
        let port = pruefserver(|pfad| match pfad {
            "/text" => (200, "Content-Type: text/plain\r\n", "ä".into()),
            "/fremdport" => (200, "Content-Type: text/plain\r\n", FREMD.load(std::sync::atomic::Ordering::Relaxed).to_string()),
            "/langsam" => {
                std::thread::sleep(std::time::Duration::from_millis(2000));
                (200, "Content-Type: text/plain\r\n", "spaet".into())
            }
            "/b" => (
                200,
                "Content-Type: text/html\r\nCache-Control: no-store\r\n",
                "<html><body><script>addEventListener('load', () => setTimeout(() => { console.log('b laenge', history.length); history.back(); }, 300));</script></body></html>".into(),
            ),
            _ if BESUCHE_A.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 0 => (
                200,
                "Content-Type: text/html\r\nCache-Control: no-store\r\n",
                r#"<html><body><script>
console.group('g'); console.log('innen'); console.groupEnd(); console.log('aussen');
console.count('z'); console.count('z');
performance.mark('m1');
console.log('marken', performance.getEntriesByType('mark').length, performance.measure('dauer', 'm1').entryType, performance.getEntriesByName('dauer').length);
const mm = matchMedia('(max-width: 800px)'); mm.addListener(() => {});
console.log('medien', mm.matches, matchMedia('(min-width: 800px)').matches, matchMedia('screen and (orientation: landscape), print').matches, matchMedia('not (hover: hover)').matches);
const x = new XMLHttpRequest();
x.open('GET', '/text');
x.overrideMimeType('text/plain; charset=x-user-defined');
x.onload = () => {
  console.log('binaer', x.responseText.length, x.responseText.charCodeAt(0), x.responseText.charCodeAt(1));
};
// Cookies aus Skripten, CORS mit Cookies, Abbrechen, defaultValue
async function pruefen() {
  document.cookie = 'hb=2; path=/';
  document.cookie = 'ha=1; unbekannt';
  console.log('cookie', document.cookie.includes('hb=2'), document.cookie.includes('ha=1'));
  const f = 'http://127.0.0.1:' + await (await fetch('/fremdport')).text();
  for (const p of ['stern', 'genau', 'genau_ohne']) {
    try { await (await fetch(f + '/' + p)).text(); console.log('cors', p, 'frei'); } catch (e) { console.log('cors', p, 'blockiert'); }
  }
  const ac = new AbortController(); let gemeldet = false;
  ac.signal.addEventListener('abort', () => { gemeldet = true; });
  const t0 = performance.now();
  const p = fetch('/langsam', { signal: ac.signal });
  setTimeout(() => ac.abort(), 100);
  try { await p; console.log('abbruch nein'); } catch (e) { console.log('abbruch', e.name, gemeldet, performance.now() - t0 < 1000); }
  const i = document.createElement('input'); document.body.appendChild(i);
  i.defaultValue = 'x'; console.log('vorgabe', i.value, i.defaultValue);
  document.cookie = 'hb=; max-age=0; path=/';
}
const fertig = pruefen().catch((e) => console.log('pruefen fehler', e));
// NetSurf nimmt eine Seite erst in den Verlauf, wenn sie fertig ist
addEventListener('load', () => fertig.then(() => setTimeout(() => { console.log('a laenge', history.length); location.href = '/b'; }, 300)));
x.send();
</script></body></html>"#
                    .into(),
            ),
            _ => (
                200,
                "Content-Type: text/html\r\nCache-Control: no-store\r\n",
                "<html><body><script>console.log('a wieder', history.length);</script></body></html>".into(),
            ),
        });
        HAUPT.store(port, std::sync::atomic::Ordering::Relaxed);
        let (r, _bild) = lauf_mit(&format!("http://127.0.0.1:{port}/a"), 640, 360, 6000, true).expect("Bild");
        assert_eq!(r, 0, "Frontend endet normal");
        let k = js_konsole(true);
        println!("{k}");
        for z in [
            "log: g\n",
            "log:   innen\n",
            "log: aussen\n",
            "log: z: 1\n",
            "log: z: 2\n",
            "log: marken 1 measure 1\n",
            "log: medien true false true true\n",
            "log: binaer 2 195 164\n",
            "log: cookie true false\n",
            "log: cors stern blockiert\n",
            "log: cors genau frei\n",
            "log: cors genau_ohne blockiert\n",
            "log: abbruch AbortError true true\n",
            "log: vorgabe x x\n",
            "log: a laenge 1\n",
            "log: b laenge 2\n",
            "log: a wieder 2\n",
        ] {
            assert!(k.contains(z), "fehlt in der Konsole: {z:?}");
        }
    }

    /// B7: Auswahlliste mit Skript – value/selectedIndex aus dem Skript
    /// ziehen NetSurfs Liste nach (Anzeige und Markierung im Menü), Auswahl
    /// per Tippen meldet „input“ und „change“.
    #[test]
    fn javascript_auswahl() {
        c(javascript_auswahl_innen);
    }

    fn javascript_auswahl_innen() {
        if im_eigenen_prozess("javascript_auswahl") {
            return;
        }
        ressourcen();
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_TEST_EINGABE", "");
            }
        }
        let _z = Zuruecksetzen;
        js_konsole(true);
        let html = r#"<html><body style="margin:0;background:white">
<select id="s" style="display:block;width:200px"><option value="1">eins<option value="2" selected>zwei<option value="3">drei</select>
<script>
const s = document.getElementById('s');
s.addEventListener('input', () => console.log('auswahl input', s.value));
s.addEventListener('change', () => console.log('auswahl change', s.value, s.selectedIndex));
setTimeout(() => { s.value = '3'; console.log('gesetzt', s.value, s.selectedIndex, s.options[2].selected); }, 300);
setTimeout(() => { s.selectedIndex = 2; console.log('index', s.value); }, 1800);
</script></body></html>"#;
        // Liste ab y 30: Menü öffnet darunter, Einträge bei y 69/88/108
        // (Bild beim Erkunden). „eins“ wählen, später Menü offen lassen.
        assert!(c_umgebung("NS_TEST_EINGABE", "1000:50:42;1400:40:69;2200:50:42"));
        let (r, bild) = lauf_mit(&data_url(html), 800, 480, 2700, true).expect("Bild");
        assert_eq!(r, 0, "Frontend endet normal");
        let k = js_konsole(true);
        println!("{k}");
        for z in ["log: gesetzt 3 2 true\n", "log: auswahl input 1\n", "log: auswahl change 1 0\n", "log: index 3\n"] {
            assert!(k.contains(z), "fehlt in der Konsole: {z:?}");
        }
        // Menü offen: die markierte Zeile (blau) ist „drei“ (vom Skript), nicht „eins“
        let px = |x: usize, y: usize| bild.px[y * bild.breite as usize + x] & 0xFF_FFFF;
        let blau = |c: u32| (c & 0xFF) > 0xC0 && ((c >> 16) & 0xFF) < 0xA0;
        assert!(blau(px(150, 108)), "„drei“ markiert: {:06x}", px(150, 108));
        assert!(!blau(px(150, 69)), "„eins“ nicht markiert: {:06x}", px(150, 69));
    }

    /// B7: localStorage dauerhaft je Herkunft – Seite 1 schreibt und geht
    /// weiter, Seite 2 (neue JS-Laufzeit) liest den Wert aus der Datei im
    /// Datenordner; mehr als 512 KB → QuotaExceededError.
    #[test]
    fn javascript_localstorage() {
        c(javascript_localstorage_innen);
    }

    fn javascript_localstorage_innen() {
        if im_eigenen_prozess("javascript_localstorage") {
            return;
        }
        ressourcen();
        js_konsole(true);
        let ordner = std::env::temp_dir().join(format!("ns_ls_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).unwrap();
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_DATEN_ORDNER", "");
            }
        }
        let _z = Zuruecksetzen;
        assert!(c_umgebung("NS_DATEN_ORDNER", &ordner.to_string_lossy().replace('\\', "/")));
        let port = pruefserver(|pfad| match pfad {
            "/ls1" => (
                200,
                "Content-Type: text/html\r\n",
                r#"<html><body><script>
console.log('ls1 vorher', localStorage.getItem('wert'));
localStorage.setItem('wert', 'gespeichert');
localStorage.setItem('zahl', 7);
try { localStorage.setItem('gross', 'x'.repeat(600 * 1024)); console.log('FALSCH: kein Fehler'); }
catch (e) { console.log('quota', e.name, localStorage.getItem('gross')); }
sessionStorage.setItem('kurz', '1');
setTimeout(() => { location.href = '/ls2'; }, 100);
</script></body></html>"#
                    .into(),
            ),
            "/ls2" => (
                200,
                "Content-Type: text/html\r\n",
                r#"<html><body><script>
console.log('ls2', localStorage.getItem('wert'), localStorage.getItem('zahl'), localStorage.length, sessionStorage.getItem('kurz'));
</script></body></html>"#
                    .into(),
            ),
            _ => (404, "Content-Type: text/plain\r\n", "fehlt".into()),
        });
        let (r, _bild) = lauf_mit(&format!("http://127.0.0.1:{port}/ls1"), 480, 320, 3000, true).expect("Bild");
        assert_eq!(r, 0, "Frontend endet normal");
        let k = js_konsole(true);
        println!("{k}");
        for z in ["log: ls1 vorher null", "log: quota QuotaExceededError null", "log: ls2 gespeichert 7 2 null"] {
            assert!(k.contains(z), "fehlt in der Konsole: {z}");
        }
        assert!(!k.contains("FALSCH"), "{k}");
        // eine Datei je Herkunft im Datenordner
        let dateien: Vec<_> = std::fs::read_dir(ordner.join("speicher")).expect("Ordner speicher").flatten().collect();
        assert_eq!(dateien.len(), 1, "eine Datei: {:?}", dateien.iter().map(|d| d.file_name()).collect::<Vec<_>>());
        let inhalt = std::fs::read_to_string(dateien[0].path()).unwrap();
        assert!(inhalt.contains("gespeichert") && !inhalt.contains("xxxx"), "{inhalt}");
    }

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

    /// B2: Adressleiste – Adresse oder Suche
    #[test]
    fn eingabe_adresse_oder_suche() {
        c(eingabe_adresse_oder_suche_innen);
    }

    fn eingabe_adresse_oder_suche_innen() {
        for (text, art, rein) in [
            ("esp32", 0, "esp32"),
            ("was ist esp32", 0, "was ist esp32"),
            ("1.5", 0, "1.5"),
            ("wikipedia.org", 2, "wikipedia.org"),
            ("  de.wikipedia.org/wiki/ESP32 ", 2, "de.wikipedia.org/wiki/ESP32"),
            ("localhost:8080/test", 2, "localhost:8080/test"),
            ("192.168.1.10", 2, "192.168.1.10"),
            ("https://html.duckduckgo.com/html/?q=x", 1, "https://html.duckduckgo.com/html/?q=x"),
            ("about:blank", 1, "about:blank"),
            ("file:///nsres/de/start.html", 1, "file:///nsres/de/start.html"),
            ("", 0, ""),
        ] {
            assert_eq!(eingabe_art(text), (art, rein.to_string()), "Eingabe {text:?}");
        }
    }

    /// B2: Abruf über http durch NetSurf zu einem Prüfserver: Weiterleitung
    /// (302, relative Location) mit Set-Cookie, der Cookie kommt bei der
    /// nächsten Anfrage zurück → grüne Seite (sonst rot); 404 als Seite.
    #[test]
    fn abruf_ueber_http() {
        c(abruf_ueber_http_innen);
    }

    fn abruf_ueber_http_innen() {
        if im_eigenen_prozess("abruf_ueber_http") {
            return;
        }
        use std::io::{Read, Write};
        ressourcen();
        let server = std::net::TcpListener::bind("127.0.0.1:0").expect("Prüfserver");
        let port = server.local_addr().unwrap().port();
        let anfragen = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let log = anfragen.clone();
        std::thread::spawn(move || {
            for s in server.incoming().flatten() {
                let mut s = s;
                let mut kopf = Vec::new();
                let mut b = [0u8; 1];
                while !kopf.ends_with(b"\r\n\r\n") && s.read(&mut b).map(|n| n == 1).unwrap_or(false) {
                    kopf.push(b[0]);
                }
                let k = String::from_utf8_lossy(&kopf).to_string();
                log.lock().unwrap().push(k.clone());
                let pfad = k.split_whitespace().nth(1).unwrap_or("/").to_string();
                let seite = |farbe: &str| format!("<html><body style='margin:0;background:white'><div style='width:200px;height:100px;background:{farbe}'></div></body></html>");
                let antwort = match pfad.as_str() {
                    "/start" => "HTTP/1.0 302 Found\r\nLocation: /ziel\r\nSet-Cookie: probe=keks42; Path=/\r\nContent-Length: 0\r\n\r\n".to_string(),
                    "/ziel" => {
                        let farbe = if k.contains("probe=keks42") { "#00ff00" } else { "#ff0000" };
                        format!("HTTP/1.0 200 OK\r\nContent-Type: text/html\r\n\r\n{}", seite(farbe))
                    }
                    _ => format!("HTTP/1.0 404 Not Found\r\nContent-Type: text/html\r\n\r\n{}", seite("#0000ff")),
                };
                let _ = s.write_all(antwort.as_bytes());
            }
        });
        let (r, bild) = lauf(&format!("http://127.0.0.1:{port}/start"), 480, 320, 3000).expect("Bild");
        assert_eq!(r, 0);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
        std::fs::write(dir.join("b2_pc_abruf.png"), spatz::image::encode_png(bild.breite, bild.hoehe, &bild.px)).unwrap();
        let a = anfragen.lock().unwrap().clone();
        println!("Anfragen: {a:#?}");
        assert!(a.iter().any(|k| k.starts_with("GET /start ")), "{a:?}");
        let ziel = a.iter().find(|k| k.starts_with("GET /ziel ")).expect("Weiterleitung befolgt");
        assert!(ziel.contains("Cookie: probe=keks42"), "Cookie mitgeschickt: {ziel}");
        assert!(ziel.contains("NetSurf/3.11"), "{ziel}");
        let gruen = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0x00_FF00).count();
        let rot = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0xFF_0000).count();
        assert!(gruen >= 200 * 100 * 9 / 10 && rot == 0, "grün {gruen}, rot {rot}");
    }

    /// B2: Startseite (eigene Ressource) mit grossem Suchfeld
    #[test]
    fn startseite_mit_suchfeld() {
        c(startseite_mit_suchfeld_innen);
    }

    fn startseite_mit_suchfeld_innen() {
        if im_eigenen_prozess("startseite_mit_suchfeld") {
            return;
        }
        ressourcen();
        let (r, bild) = lauf("resource:welcome.html", 800, 480, 1500).expect("Bild");
        assert_eq!(r, 0);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
        // ohne Werkzeugleiste (die Adresszeile zeigt den lokalen Pfad)
        let oben = 32 * bild.breite as usize;
        std::fs::write(dir.join("b2_pc_startseite.png"), spatz::image::encode_png(bild.breite, bild.hoehe - 32, &bild.px[oben..])).unwrap();
        // Kopfbalken (#3a6ea5) und weisses Suchfeld mit blauem Rand
        let blau = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0x3a_6ea5).count();
        assert!(blau > 800 * 40, "Kopf/Rahmen blau: {blau}");
    }

    /// Echte Seite über den PC-Proxy (werkzeuge/proxy.py) nachstellen – nur
    /// auf Anforderung, braucht Netz bzw. eine Ablage:
    /// `NS_PROXY=127.0.0.1:8899 NS_SEITE=https://… [NS_MS=60000]
    /// cargo test --release -- --ignored --nocapture seite_ueber_proxy`.
    /// Bild nach esp32/target/seite_ueber_proxy.png, JS-Ausgaben auf die Konsole.
    #[test]
    #[ignore]
    fn seite_ueber_proxy() {
        c(seite_ueber_proxy_innen);
    }

    fn seite_ueber_proxy_innen() {
        if im_eigenen_prozess("seite_ueber_proxy") {
            return;
        }
        let proxy = std::env::var("NS_PROXY").expect("NS_PROXY=host:port");
        let seite = std::env::var("NS_SEITE").expect("NS_SEITE=Adresse");
        let ms: i32 = std::env::var("NS_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(60_000);
        ressourcen();
        assert!(c_umgebung("NS_PROXY", &proxy));
        js_konsole(true);
        // NS_GROESSE=„BxH“ (z. B. die Fenstergrösse am Board), sonst 800×480
        let (b, h) = std::env::var("NS_GROESSE")
            .ok()
            .and_then(|g| {
                let (b, h) = g.split_once('x')?;
                Some((b.parse().ok()?, h.parse().ok()?))
            })
            .unwrap_or((800, 480));
        let t = std::time::Instant::now();
        let (r, bild) = lauf_mit(&seite, b, h, ms, true).expect("Bild");
        println!("Ende {r} nach {:?}, Neu-Layouts {}", t.elapsed(), neu_layouts());
        #[cfg(target_pointer_width = "32")]
        {
            let (aufrufe, schritte) = kern::heap::zaehler(false);
            println!("Heap: {aufrufe} Aufrufe, {schritte} Schritte, {} je Aufruf", schritte / aufrufe.max(1));
        }
        println!("JS:
{}", js_konsole(true));
        let ziel = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/seite_ueber_proxy.png");
        std::fs::write(&ziel, spatz::image::encode_png(bild.breite, bild.hoehe, &bild.px)).unwrap();
        println!("Bild: {}", ziel.display());
    }

    /// Rückfallebene (Nutzer 22.11.): ein Skript sprengt die JS-Speichergrenze
    /// → JavaScript wird für die Seite abgeschaltet, sie wird ohne weitere
    /// Skripte angezeigt: das zweite Skript (rot) läuft nicht mehr.
    #[test]
    fn ohne_js_rueckfall_speicher() {
        c(ohne_js_rueckfall_speicher_innen);
    }

    fn ohne_js_rueckfall_speicher_innen() {
        if im_eigenen_prozess("ohne_js_rueckfall_speicher") {
            return;
        }
        ressourcen();
        js_konsole(true);
        let html = r#"<html><body style="margin:0;background:white">
<div style="width:200px;height:100px;background:#0000ff"></div>
<div id="r"></div>
<script>
const a = []; while (true) a.push(new Array(1000).fill(1));
</script>
<script>
document.getElementById('r').innerHTML = '<div style="width:200px;height:100px;background:#ff0000"></div>';
</script></body></html>"#;
        let (r, bild) = lauf_mit(&data_url(html), 480, 320, 4000, true).expect("Bild");
        assert_eq!(r, 0);
        let k = js_konsole(true);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
        std::fs::write(dir.join("b2_pc_ohne_js.png"), spatz::image::encode_png(bild.breite, bild.hoehe, &bild.px)).unwrap();
        // „Fehler: null“ = kein Speicher mehr für das Fehlerobjekt (B10)
        assert!(k.contains("out of memory") || k.contains("Fehler: null"), "{k}");
        assert_eq!(ohne_js_rueckfaelle(), 1, "ein Rückfall");
        let blau = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0x00_00FF).count();
        let rot = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0xFF_0000).count();
        assert!(blau >= 200 * 100 * 9 / 10 && rot == 0, "blau {blau}, rot {rot}");
    }

    /// Rückfallebene, Zeitgrenze: erscheint eine Seite nicht rechtzeitig
    /// (hier: Skript vom Prüfserver kommt erst nach 300 ms, Grenze 1 ms per
    /// NS_RUECKFALL_MS), wird JavaScript für sie abgeschaltet – das Skript
    /// läuft nicht mehr, die Seite wird ohne JavaScript neu geholt (B10) und
    /// zeigt ihr `<noscript>`.
    #[test]
    fn ohne_js_rueckfall_zeit() {
        c(ohne_js_rueckfall_zeit_innen);
    }

    fn ohne_js_rueckfall_zeit_innen() {
        if im_eigenen_prozess("ohne_js_rueckfall_zeit") {
            return;
        }
        use std::io::{Read, Write};
        ressourcen();
        // gilt nur für diesen Test (i686: alle Tests in einem Prozess)
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_RUECKFALL_MS", "");
            }
        }
        let _zurueck = Zuruecksetzen;
        assert!(c_umgebung("NS_RUECKFALL_MS", "1"));
        let server = std::net::TcpListener::bind("127.0.0.1:0").expect("Prüfserver");
        let port = server.local_addr().unwrap().port();
        let anfragen = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let log = anfragen.clone();
        std::thread::spawn(move || {
            for s in server.incoming().flatten() {
                let mut s = s;
                let mut kopf = Vec::new();
                let mut b = [0u8; 1];
                while !kopf.ends_with(b"\r\n\r\n") && s.read(&mut b).map(|n| n == 1).unwrap_or(false) {
                    kopf.push(b[0]);
                }
                let k = String::from_utf8_lossy(&kopf).to_string();
                let pfad = k.split_whitespace().nth(1).unwrap_or("/").to_string();
                log.lock().unwrap().push(pfad.clone());
                let antwort = match pfad.as_str() {
                    "/seite" => "HTTP/1.0 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body style='margin:0;background:white'>\
<noscript><div style='width:200px;height:100px;background:#0000ff'></div></noscript><div id='r'></div>\
<script src='/langsam.js'></script></body></html>"
                        .to_string(),
                    "/langsam.js" => {
                        std::thread::sleep(std::time::Duration::from_millis(300));
                        "HTTP/1.0 200 OK\r\nContent-Type: text/javascript\r\n\r\n\
document.getElementById('r').innerHTML = '<div style=\"width:200px;height:100px;background:#ff0000\"></div>';"
                            .to_string()
                    }
                    _ => "HTTP/1.0 404 Not Found\r\nContent-Type: text/plain\r\n\r\nweg".to_string(),
                };
                let _ = s.write_all(antwort.as_bytes());
            }
        });
        let (r, bild) = lauf_mit(&format!("http://127.0.0.1:{port}/seite"), 480, 320, 5000, true).expect("Bild");
        assert_eq!(r, 0);
        let a = anfragen.lock().unwrap().clone();
        assert_eq!(ohne_js_rueckfaelle(), 1, "ein Rückfall; Anfragen {a:?}");
        // B10: die Seite wird ohne JavaScript neu geholt, das Skript nicht
        assert_eq!(a.iter().filter(|p| p.as_str() == "/seite").count(), 2, "Seite ohne JS neu geholt: {a:?}");
        assert_eq!(a.iter().filter(|p| p.as_str() == "/langsam.js").count(), 1, "Skript einmal geholt: {a:?}");
        let rot = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0xFF_0000).count();
        assert_eq!(rot, 0, "Skript lief nicht mehr");
        let blau = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0x00_00FF).count();
        assert!(blau >= 200 * 100, "<noscript> der neu geladenen Seite sichtbar ({blau} px)");
    }

    /// Seite auf der Liste „ohne JavaScript“ (Knopf „Ohne JS“, Neuladen):
    /// Skripte laufen nicht, `<noscript>` wird angezeigt.
    #[test]
    fn ohne_js_liste() {
        c(ohne_js_liste_innen);
    }

    fn ohne_js_liste_innen() {
        if im_eigenen_prozess("ohne_js_liste") {
            return;
        }
        ressourcen();
        let html = r#"<html><body style="margin:0;background:white">
<noscript><div style="width:200px;height:100px;background:#0000ff"></div></noscript>
<div id="r"></div>
<script>document.getElementById('r').innerHTML = '<div style="width:200px;height:100px;background:#ff0000"></div>';</script>
</body></html>"#;
        let adresse = data_url(html);
        assert!(ohne_js_setzen(&adresse));
        let (r, bild) = lauf_mit(&adresse, 480, 320, 1500, true).expect("Bild");
        assert_eq!(r, 0);
        let blau = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0x00_00FF).count();
        let rot = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0xFF_0000).count();
        assert!(blau >= 200 * 100 * 9 / 10 && rot == 0, "blau {blau}, rot {rot}");
    }

    /// B3: Zwischenspeicher hält die Grenze. Sechs Seiten zu je 700 KB,
    /// zwischenspeicherbar (max-age), per meta refresh nacheinander geladen
    /// – 4,2 MB durch einen Zwischenspeicher mit 2 MB Grenze. Danach liegt
    /// der freigebbare Teil (Seiten ohne Nutzer) innerhalb der Grenze.
    #[test]
    fn zwischenspeicher_grenze() {
        c(zwischenspeicher_grenze_innen);
    }

    fn zwischenspeicher_grenze_innen() {
        if im_eigenen_prozess("zwischenspeicher_grenze") {
            return;
        }
        use std::io::{Read, Write};
        ressourcen();
        const GRENZE: u32 = 2 * 1024 * 1024;
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_CACHE_GRENZE", "");
            }
        }
        let _zurueck = Zuruecksetzen;
        assert!(c_umgebung("NS_CACHE_GRENZE", &GRENZE.to_string()));
        let server = std::net::TcpListener::bind("127.0.0.1:0").expect("Prüfserver");
        let port = server.local_addr().unwrap().port();
        let anfragen = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let log = anfragen.clone();
        std::thread::spawn(move || {
            for s in server.incoming().flatten() {
                let mut s = s;
                let mut kopf = Vec::new();
                let mut b = [0u8; 1];
                while !kopf.ends_with(b"\r\n\r\n") && s.read(&mut b).map(|n| n == 1).unwrap_or(false) {
                    kopf.push(b[0]);
                }
                let k = String::from_utf8_lossy(&kopf).to_string();
                let pfad = k.split_whitespace().nth(1).unwrap_or("/").to_string();
                log.lock().unwrap().push(pfad.clone());
                let n: usize = pfad.trim_start_matches("/seite/").parse().unwrap_or(0);
                let weiter = if n < 6 { format!("<meta http-equiv=\"refresh\" content=\"0;url=/seite/{}\">", n + 1) } else { String::new() };
                let fuell = "x".repeat(700 * 1024);
                let html = format!("<html><head>{weiter}</head><body style='margin:0;background:white'><p>Seite {n}</p><!-- {fuell} --></body></html>");
                let antwort = format!(
                    "HTTP/1.0 200 OK\r\nContent-Type: text/html\r\nCache-Control: max-age=3600\r\nContent-Length: {}\r\n\r\n{html}",
                    html.len()
                );
                let _ = s.write_all(antwort.as_bytes());
            }
        });
        let (r, _bild) = lauf_mit(&format!("http://127.0.0.1:{port}/seite/1"), 480, 320, 6000, false).expect("Bild");
        assert_eq!(r, 0);
        let a = anfragen.lock().unwrap().clone();
        let seiten = a.iter().filter(|p| p.starts_with("/seite/")).count();
        let (gesamt, grenze, frei) = zwischenspeicher();
        println!("Seiten {seiten}, Zwischenspeicher {gesamt} B, Grenze {grenze} B, freigebbar {frei} B");
        assert!(seiten >= 6, "alle Seiten geladen: {a:?}");
        // NetSurf teilt die Grenze: 25 % Bild-Zwischenspeicher, 75 % für die
        // Quelldaten (llcache) – desktop/netsurf.c
        let quelle = GRENZE - GRENZE / 4;
        assert_eq!(grenze, quelle, "Grenze übernommen");
        assert!(frei <= quelle, "freigebbarer Teil {frei} B über der Grenze {quelle} B");
        // es wurde wirklich aufgeräumt: 6 × 700 KB lagen nicht alle im Speicher
        assert!(gesamt < 6 * 700 * 1024, "Zwischenspeicher {gesamt} B");
    }

    /// B4: Lesezeichen, Verlauf und Adressleiste per Antippen und Tippen
    /// (Eingabe-Skript der Prüf-Fläche): Seite A merken, Link zu B
    /// antippen, in der Adressleiste die Adresse von C tippen, „Liste“
    /// antippen, dort das Lesezeichen A antippen. Danach liegen Lesezeichen
    /// und Adressen im Datenordner.
    #[test]
    fn lesezeichen_verlauf_adressleiste() {
        c(lesezeichen_verlauf_adressleiste_innen);
    }

    fn lesezeichen_verlauf_adressleiste_innen() {
        if im_eigenen_prozess("lesezeichen_verlauf_adressleiste") {
            return;
        }
        use std::io::{Read, Write};
        ressourcen();
        let ordner = std::env::temp_dir().join(format!("ns_daten_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).unwrap();
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_DATEN_ORDNER", "");
                c_umgebung("NS_TEST_EINGABE", "");
            }
        }
        let _zurueck = Zuruecksetzen;
        // ohne Laufwerk: NetSurfs file:-Abrufer kennt nur Pfade wie /a/b
        // (Windows nimmt sie auf dem aktuellen Laufwerk)
        let ohne_lw = |p: &str| p.replace('\\', "/").trim_start_matches(|c: char| c.is_ascii_alphabetic()).trim_start_matches(':').to_string();
        assert!(c_umgebung("NS_DATEN_ORDNER", &ohne_lw(&ordner.to_string_lossy())));
        let server = std::net::TcpListener::bind("127.0.0.1:0").expect("Prüfserver");
        let port = server.local_addr().unwrap().port();
        let anfragen = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let log = anfragen.clone();
        std::thread::spawn(move || {
            for s in server.incoming().flatten() {
                let mut s = s;
                let mut kopf = Vec::new();
                let mut b = [0u8; 1];
                while !kopf.ends_with(b"\r\n\r\n") && s.read(&mut b).map(|n| n == 1).unwrap_or(false) {
                    kopf.push(b[0]);
                }
                let k = String::from_utf8_lossy(&kopf).to_string();
                let pfad = k.split_whitespace().nth(1).unwrap_or("/").to_string();
                log.lock().unwrap().push(pfad.clone());
                let (titel, farbe, mehr) = match pfad.as_str() {
                    "/a" => ("Seite A", "#00ff00", "<a href='/b' style='display:block;width:300px;height:60px;background:#0000ff'></a>"),
                    "/b" => ("Seite B", "#ffff00", ""),
                    "/c" => ("Seite C", "#00ffff", ""),
                    _ => ("weg", "#ff0000", ""),
                };
                let html = format!(
                    "<html><head><title>{titel}</title></head><body style='margin:0;background:white'>\
                     <div style='width:200px;height:100px;background:{farbe}'></div><div style='height:30px'></div>{mehr}</body></html>"
                );
                let antwort = format!("HTTP/1.0 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{html}", html.len());
                let _ = s.write_all(antwort.as_bytes());
            }
        });
        // NS_SKRIPT_KUERZEN=n: nur die ersten n Schritte (Fehlersuche).
        // Merken (238,15) – Link zu B – Adressleiste antippen (leert sie) und
        // Adresse von C tippen – Liste (300,15)
        let schritte = [
            "1000:238:15".to_string(),
            "1500:150:160".to_string(),
            "2500:600:15".to_string(),
            format!("2800:T:http://127.0.0.1:{port}/c\\n"),
            // zweite Eingabe gleich danach: die Leiste muss wieder leer sein
            "3500:600:15".to_string(),
            format!("3700:T:http://127.0.0.1:{port}/d\\n"),
            "4500:300:15".to_string(),
        ];
        let n = std::env::var("NS_SKRIPT_KUERZEN").ok().and_then(|v| v.parse().ok()).unwrap_or(schritte.len());
        let skript = schritte[..n.min(schritte.len())].join(";");
        assert!(c_umgebung("NS_TEST_EINGABE", &skript));
        let (r, bild) = lauf_mit(&format!("http://127.0.0.1:{port}/a"), 800, 480, 6000, false).expect("Bild");
        assert_eq!(r, 0);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
        // ohne Werkzeugleiste (die Adresszeile kann lokale Pfade zeigen)
        let oben = 32 * bild.breite as usize;
        std::fs::write(dir.join("b4_pc_liste.png"), spatz::image::encode_png(bild.breite, bild.hoehe - 32, &bild.px[oben..])).unwrap();
        let a = anfragen.lock().unwrap().clone();
        println!("Anfragen {a:?}");
        let liste = std::fs::read_to_string(ordner.join("liste.html")).expect("liste.html");
        println!("{liste}");
        assert!(a.iter().any(|p| p == "/b"), "Link angetippt: {a:?}");
        assert!(a.iter().any(|p| p == "/c"), "Adressleiste: {a:?}");
        if n >= 5 {
            assert!(a.iter().any(|p| p == "/d"), "zweite Eingabe in der Adressleiste: {a:?}");
        }
        if n < schritte.len() {
            return;
        }
        // Liste: Lesezeichen A, Verlauf neueste zuerst C, B, A
        let (lz, verlauf) = liste.split_once("<h2>Verlauf</h2>").expect("Abschnitt Verlauf");
        assert!(lz.contains(&format!("127.0.0.1:{port}/a\">Seite A")), "Lesezeichen A: {lz}");
        let pos = |t: &str| verlauf.find(t).unwrap_or(usize::MAX);
        assert!(pos("Seite C") < pos("Seite B") && pos("Seite B") < pos("Seite A") && pos("Seite A") != usize::MAX, "Verlauf: {verlauf}");
        // gespeichert beim Beenden
        let datei = std::fs::read_to_string(ordner.join("Lesezeichen")).expect("Datei Lesezeichen");
        assert!(datei.contains(&format!("127.0.0.1:{port}/a")), "{datei}");
        assert!(ordner.join("Adressen").exists(), "Datei Adressen");

        // zweiter Start (eigener Prozess, NetSurf startet nur einmal je
        // Prozess): Liste aus den gespeicherten Dateien, Lesezeichen A
        // antippen → Seite A. Nur 64 Bit – i686 hat alle Tests in einem
        // Programm.
        if cfg!(target_pointer_width = "64") {
            anfragen.lock().unwrap().clear();
            let aus = std::process::Command::new(std::env::current_exe().expect("Testprogramm"))
                .args(["tests::lesezeichen_zweiter_start", "--exact", "--test-threads=1", "--nocapture"])
                .env("NS_EINZELN", "1")
                .env("NS_ZWEITER_ORDNER", ohne_lw(&ordner.to_string_lossy()))
                .env("NS_ZWEITER_PORT", port.to_string())
                .output()
                .expect("Kindprozess");
            let text = format!("{}{}", String::from_utf8_lossy(&aus.stdout), String::from_utf8_lossy(&aus.stderr));
            assert!(aus.status.success() && text.contains("1 passed"), "zweiter Start:\n{text}");
            let a = anfragen.lock().unwrap().clone();
            assert!(a.iter().any(|p| p == "/a"), "Lesezeichen geöffnet: {a:?}");
        }
        let _ = std::fs::remove_dir_all(&ordner);
    }

    /// Teil von `lesezeichen_verlauf_adressleiste` (zweiter Start, eigener
    /// Prozess); allein aufgerufen tut er nichts.
    #[test]
    fn lesezeichen_zweiter_start() {
        c(lesezeichen_zweiter_start_innen);
    }

    fn lesezeichen_zweiter_start_innen() {
        let (Ok(ordner), Ok(port)) = (std::env::var("NS_ZWEITER_ORDNER"), std::env::var("NS_ZWEITER_PORT")) else {
            return;
        };
        ressourcen();
        assert!(c_umgebung("NS_DATEN_ORDNER", &ordner));
        // „Liste“ antippen, dann das erste Lesezeichen (Seite A)
        assert!(c_umgebung("NS_TEST_EINGABE", "1000:300:15;2500:300:157"));
        let (r, bild) = lauf_mit(&format!("http://127.0.0.1:{port}/c"), 800, 480, 4000, false).expect("Bild");
        assert_eq!(r, 0);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
        let oben = 32 * bild.breite as usize;
        std::fs::write(dir.join("b4_pc_aus_liste.png"), spatz::image::encode_png(bild.breite, bild.hoehe - 32, &bild.px[oben..])).unwrap();
        let gruen = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0x00_FF00).count();
        assert!(gruen >= 200 * 100 * 9 / 10, "Seite A aus der Liste geöffnet: grün {gruen}");
    }

    /// Seite aus drei Blöcken (rot, grün, blau, je 400 × 300) für B5
    fn b5_seite() -> String {
        data_url(
            "<html><body style='margin:0;background:white'>\
             <div style='width:400px;height:300px;background:#ff0000'></div>\
             <div style='width:400px;height:300px;background:#00ff00'></div>\
             <div style='width:400px;height:300px;background:#0000ff'></div>\
             <div style='height:600px'></div></body></html>",
        )
    }

    /// PNG des Bildes, Adressfeld weiss (Prüfserver mit wechselndem Port –
    /// das Erwartungsbild soll sich nicht bei jedem Lauf ändern).
    fn ohne_adresse(bild: &Bild) -> Vec<u8> {
        let mut px = bild.px.clone();
        let b = bild.breite as usize;
        for y in 3..27.min(bild.hoehe as usize) {
            for x in 334..(b.saturating_sub(92)) {
                px[y * b + x] = 0xFF_FFFF;
            }
        }
        // Statuszeile (zeigt beim Zeigen auf Links die Adresse)
        let h = bild.hoehe as usize;
        for y in h.saturating_sub(17)..h {
            for x in 0..b.saturating_sub(270) {
                px[y * b + x] = 0xDD_DDDD;
            }
        }
        spatz::image::encode_png(bild.breite, bild.hoehe, &px)
    }

    fn b5_farbe(bild: &Bild, x: usize, y: usize) -> u32 {
        bild.px[y * bild.breite as usize + x] & 0xFF_FFFF
    }

    /// B5: Wischen scrollt um den Fingerweg (300 px → oben grün, darunter
    /// blau); mit dem alten festen Schritt (100 px) stünde oben noch Rot.
    #[test]
    fn touch_scrollen() {
        c(touch_scrollen_innen);
    }

    fn touch_scrollen_innen() {
        if im_eigenen_prozess("touch_scrollen") {
            return;
        }
        ressourcen();
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_TEST_EINGABE", "");
            }
        }
        let _z = Zuruecksetzen;
        // drei Wischbewegungen 120 + 120 + 60 px (Pixel werden gesammelt)
        assert!(c_umgebung("NS_TEST_EINGABE", "800:S:120:200:200;850:S:120:200:200;900:S:60:200:200"));
        let (r, bild) = lauf_mit(&b5_seite(), 800, 480, 2000, false).expect("Bild");
        assert_eq!(r, 0);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
        std::fs::write(dir.join("b5_pc_gescrollt.png"), spatz::image::encode_png(bild.breite, bild.hoehe, &bild.px)).unwrap();
        // Seitenbereich beginnt unter der Werkzeugleiste (y = 30)
        assert_eq!(b5_farbe(&bild, 100, 40), 0x00_FF00, "oben grün (300 px gescrollt)");
        assert_eq!(b5_farbe(&bild, 100, 340), 0x00_00FF, "darunter blau");
    }

    /// B5: Pinch-Zoom verdoppelt die Seite (roter Block reicht bis x 800).
    #[test]
    fn touch_zoomen() {
        c(touch_zoomen_innen);
    }

    fn touch_zoomen_innen() {
        if im_eigenen_prozess("touch_zoomen") {
            return;
        }
        ressourcen();
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_TEST_EINGABE", "");
            }
        }
        let _z = Zuruecksetzen;
        assert!(c_umgebung("NS_TEST_EINGABE", "800:Z:200"));
        let (r, bild) = lauf_mit(&b5_seite(), 800, 480, 2000, false).expect("Bild");
        assert_eq!(r, 0);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
        std::fs::write(dir.join("b5_pc_gezoomt.png"), spatz::image::encode_png(bild.breite, bild.hoehe, &bild.px)).unwrap();
        // ohne Zoom wäre bei x 600 Weiss (Block 400 breit)
        assert_eq!(b5_farbe(&bild, 600, 100), 0xFF_0000, "rot bis über x 600 (Faktor 2)");
    }

    /// B6: Formularfeld antippen → die Schreibmarke meldet den Fokus samt
    /// Lage an die Bildschirmtastatur; Text tippen, Eingabetaste schickt das
    /// Formular ab.
    #[test]
    fn formular_tastatur() {
        c(formular_tastatur_innen);
    }

    fn formular_tastatur_innen() {
        if im_eigenen_prozess("formular_tastatur") {
            return;
        }
        use std::io::{Read, Write};
        ressourcen();
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_TEST_EINGABE", "");
            }
        }
        let _z = Zuruecksetzen;
        let server = std::net::TcpListener::bind("127.0.0.1:0").expect("Prüfserver");
        let port = server.local_addr().unwrap().port();
        let anfragen = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let log = anfragen.clone();
        std::thread::spawn(move || {
            for s in server.incoming().flatten() {
                let mut s = s;
                let mut kopf = Vec::new();
                let mut b = [0u8; 1];
                while !kopf.ends_with(b"\r\n\r\n") && s.read(&mut b).map(|n| n == 1).unwrap_or(false) {
                    kopf.push(b[0]);
                }
                let k = String::from_utf8_lossy(&kopf).to_string();
                let pfad = k.split_whitespace().nth(1).unwrap_or("/").to_string();
                log.lock().unwrap().push(pfad.clone());
                let html = if pfad == "/" {
                    "<html><body style='margin:0;background:white'><form action='/suche' method='get'>\
                     <input name='q' style='width:300px;height:40px;font-size:24px'></form></body></html>"
                        .to_string()
                } else {
                    "<html><body style='margin:0;background:#00ff00'>Ergebnis</body></html>".to_string()
                };
                let antwort = format!("HTTP/1.0 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{html}", html.len());
                let _ = s.write_all(antwort.as_bytes());
            }
        });
        tastatur_meldungen();
        // Feld (Seite 0..300 × 0..40, unter der Werkzeugleiste ab y 30)
        assert!(c_umgebung("NS_TEST_EINGABE", "1000:100:50;1400:T:abc\\n"));
        let (r, _bild) = lauf_mit(&format!("http://127.0.0.1:{port}/"), 800, 480, 3000, false).expect("Bild");
        assert_eq!(r, 0);
        let a = anfragen.lock().unwrap().clone();
        let m = tastatur_meldungen();
        println!("Anfragen {a:?}\nTastatur {m:?}");
        assert!(a.iter().any(|p| p == "/suche?q=abc"), "Formular abgeschickt: {a:?}");
        let an = m.iter().find(|e| e.0).expect("Fokus an gemeldet");
        assert!(an.2 >= 30 && an.2 <= 30 + 45 && an.3 > 0, "Lage im Feld: {an:?}");
        assert!(m.iter().any(|e| !e.0), "Fokus wieder abgegeben: {m:?}");
    }

    /// B8: JPEG (progressiv, B0b) und PNG mit Transparenz in einer Seite –
    /// über den Dekoder des Systems (c/bild.c). Das JPEG wird auf die halbe
    /// Grösse gezeichnet, durch das PNG scheint der grüne Hintergrund.
    #[test]
    fn bilder_jpeg_png() {
        c(bilder_jpeg_png_innen);
    }

    fn bilder_jpeg_png_innen() {
        if im_eigenen_prozess("bilder_jpeg_png") {
            return;
        }
        use std::io::{Read, Write};
        ressourcen();
        let ordner = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../desktop/tests/bilder");
        let jpeg = std::fs::read(ordner.join("progressiv.jpg")).expect("progressiv.jpg");
        let png = std::fs::read(ordner.join("transparent.png")).expect("transparent.png");
        assert_eq!(jpeg.get(..2), Some(&[0xFF, 0xD8][..]));
        let server = std::net::TcpListener::bind("127.0.0.1:0").expect("Prüfserver");
        let port = server.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in server.incoming().flatten() {
                let mut s = s;
                let mut kopf = Vec::new();
                let mut b = [0u8; 1];
                while !kopf.ends_with(b"\r\n\r\n") && s.read(&mut b).map(|n| n == 1).unwrap_or(false) {
                    kopf.push(b[0]);
                }
                let k = String::from_utf8_lossy(&kopf).to_string();
                let pfad = k.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (typ, inhalt): (&str, Vec<u8>) = match pfad.as_str() {
                    "/p.jpg" => ("image/jpeg", jpeg.clone()),
                    "/t.png" => ("image/png", png.clone()),
                    _ => (
                        "text/html",
                        b"<html><body style='margin:0;background:#00ff00'>\
                          <img src='/p.jpg' width='320' height='240' style='display:block'>\
                          <img src='/t.png' style='display:block'></body></html>"
                            .to_vec(),
                    ),
                };
                let _ = s.write_all(format!("HTTP/1.0 200 OK\r\nContent-Type: {typ}\r\nContent-Length: {}\r\n\r\n", inhalt.len()).as_bytes());
                let _ = s.write_all(&inhalt);
            }
        });
        let (r, bild) = lauf_mit(&format!("http://127.0.0.1:{port}/"), 800, 480, 3000, false).expect("Bild");
        assert_eq!(r, 0);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
        std::fs::write(dir.join("b8_pc_bilder.png"), ohne_adresse(&bild)).unwrap();
        // Bildpunkt als (r, g, b) aus 0x00RRGGBB
        let rgb = |x: usize, y: usize| {
            let p = bild.px[y * bild.breite as usize + x];
            ((p >> 16) & 0xFF, (p >> 8) & 0xFF, p & 0xFF)
        };
        // Seite ab y = 30; JPEG halb so gross: rotes Feld (8..32), blaues
        // (64..88), Sonne um (246, 57)
        let (r0, g0, b0) = rgb(20, 30 + 20);
        assert!(r0 > 180 && g0 < 80 && b0 < 80, "rotes Feld im JPEG: {:?}", (r0, g0, b0));
        let (r1, g1, b1) = rgb(76, 30 + 20);
        assert!(b1 > 180 && r1 < 80 && g1 < 100, "blaues Feld im JPEG: {:?}", (r1, g1, b1));
        let (r2, g2, b2) = rgb(246, 30 + 57);
        assert!(r2 > 200 && g2 > 160 && b2 < 120, "Sonne im JPEG: {:?}", (r2, g2, b2));
        // PNG darunter (ab y = 270): Ecke durchsichtig → Grün, Rechteck blau
        assert_eq!(rgb(4, 274), (0, 255, 0), "Hintergrund scheint durchs PNG");
        let (r3, g3, b3) = rgb(50, 270 + 40);
        assert!(b3 > 180 && r3 < 100, "blaues Rechteck im PNG: {:?}", (r3, g3, b3));
    }

    /// B8: Text markieren (Ziehen mit gedrückter Taste, am Board „Halten,
    /// dann Ziehen“) legt ihn gleich in die Zwischenablage des Systems.
    #[test]
    fn text_kopieren() {
        c(text_kopieren_innen);
    }

    fn text_kopieren_innen() {
        if im_eigenen_prozess("text_kopieren") {
            return;
        }
        ressourcen();
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_TEST_EINGABE", "");
            }
        }
        let _z = Zuruecksetzen;
        let seite = data_url("<html><body style='margin:0;font-size:32px'><p style='margin:0'>Erste Zeile</p><p style='margin:0'>Hallo Welt Ablage</p></body></html>");
        // Zeile ab y = 30 (unter der Werkzeugleiste), von links über den Text
        assert!(c_umgebung("NS_TEST_EINGABE", "600:314:-23;800:D:5:82:700:82"));
        let (r, _bild) = lauf_mit(&seite, 800, 480, 2000, false).expect("Bild");
        assert_eq!(r, 0);
        let t = ablage_inhalt();
        println!("Zwischenablage: {t:?}");
        assert!(t.starts_with("Hallo Welt Ablage"), "markierter Text in der Zwischenablage: {t:?}");
    }

    /// B8: Tabs – ein Link mit target="_blank" öffnet einen zweiten Tab
    /// (blaue Seite); „Tabs 2“ öffnet die Liste, die zweite Zeile (der erste
    /// Tab, grüne Seite) wechselt zurück.
    #[test]
    fn tabs_wechseln() {
        c(tabs_wechseln_innen);
    }

    fn tabs_wechseln_innen() {
        if im_eigenen_prozess("tabs_wechseln") {
            return;
        }
        use std::io::{Read, Write};
        unsafe extern "C" {
            fn ns_tabs_hoechststand() -> i32;
        }
        ressourcen();
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_TEST_EINGABE", "");
            }
        }
        let _z = Zuruecksetzen;
        let server = std::net::TcpListener::bind("127.0.0.1:0").expect("Prüfserver");
        let port = server.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in server.incoming().flatten() {
                let mut s = s;
                let mut kopf = Vec::new();
                let mut b = [0u8; 1];
                while !kopf.ends_with(b"\r\n\r\n") && s.read(&mut b).map(|n| n == 1).unwrap_or(false) {
                    kopf.push(b[0]);
                }
                let k = String::from_utf8_lossy(&kopf).to_string();
                let pfad = k.split_whitespace().nth(1).unwrap_or("/").to_string();
                let html = if pfad == "/b" {
                    "<html><head><title>Blau</title></head><body style='margin:0;background:#0000ff'></body></html>"
                } else {
                    "<html><head><title>Gruen</title></head><body style='margin:0;background:#00ff00'>\
                     <a href='/b' target='_blank' style='display:block;width:400px;height:200px;background:#ff0000'>x</a></body></html>"
                };
                let _ = s.write_all(format!("HTTP/1.0 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{html}", html.len()).as_bytes());
            }
        });
        let farbe = |bild: &Bild, x: usize, y: usize| bild.px[y * bild.breite as usize + x] & 0xFF_FFFF;
        // Link, dann „Tabs 2“ (rechts in der Werkzeugleiste), zweite Zeile
        assert!(c_umgebung("NS_TEST_EINGABE", "1500:100:100;3000:770:15;3600:200:115"));
        let (r, bild) = lauf_mit(&format!("http://127.0.0.1:{port}/"), 800, 480, 5000, false).expect("Bild");
        assert_eq!(r, 0);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
        std::fs::write(dir.join("b8_pc_tabs.png"), ohne_adresse(&bild)).unwrap();
        assert_eq!(farbe(&bild, 600, 300), 0x00_FF00, "zurück im ersten Tab (grün)");
        assert_eq!(farbe(&bild, 100, 100), 0xFF_0000, "Link des ersten Tabs");
        // SAFETY: Zähler aus c/tabs.c, NetSurf läuft nicht mehr.
        assert_eq!(unsafe { ns_tabs_hoechststand() }, 2, "zwei Tabs");
    }

    /// B8: Download – ein Link auf eine Datei, die NetSurf nicht anzeigt
    /// (application/octet-stream, 200 KB), landet unverändert im
    /// Download-Ordner; ein zweiter Download gleichen Namens überschreibt
    /// sie nicht.
    #[test]
    fn download_speichern() {
        c(download_speichern_innen);
    }

    fn download_speichern_innen() {
        if im_eigenen_prozess("download_speichern") {
            return;
        }
        use std::io::{Read, Write};
        unsafe extern "C" {
            fn ns_downloads_fertig() -> i32;
        }
        ressourcen();
        let ordner = std::env::temp_dir().join(format!("ns_dl_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).unwrap();
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_DATEN_ORDNER", "");
                c_umgebung("NS_TEST_EINGABE", "");
            }
        }
        let _zurueck = Zuruecksetzen;
        let ohne_lw = |p: &str| p.replace('\\', "/").trim_start_matches(|c: char| c.is_ascii_alphabetic()).trim_start_matches(':').to_string();
        assert!(c_umgebung("NS_DATEN_ORDNER", &ohne_lw(&ordner.to_string_lossy())));
        let inhalt: Vec<u8> = (0..200 * 1024u32).map(|i| (i * 7 + i / 251) as u8).collect();
        let fuer_server = inhalt.clone();
        let server = std::net::TcpListener::bind("127.0.0.1:0").expect("Prüfserver");
        let port = server.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in server.incoming().flatten() {
                let mut s = s;
                let mut kopf = Vec::new();
                let mut b = [0u8; 1];
                while !kopf.ends_with(b"\r\n\r\n") && s.read(&mut b).map(|n| n == 1).unwrap_or(false) {
                    kopf.push(b[0]);
                }
                let k = String::from_utf8_lossy(&kopf).to_string();
                let pfad = k.split_whitespace().nth(1).unwrap_or("/").to_string();
                if pfad == "/kaputt" {
                    // kündigt 200 KB an, schickt 50 KB und schliesst
                    let _ = s.write_all(
                        format!(
                            "HTTP/1.0 200 OK\r\nContent-Type: application/octet-stream\r\n\
                             Content-Disposition: attachment; filename=\"kaputt.bin\"\r\nContent-Length: {}\r\n\r\n",
                            fuer_server.len()
                        )
                        .as_bytes(),
                    );
                    let _ = s.write_all(&fuer_server[..50 * 1024]);
                } else if pfad == "/datei" {
                    let _ = s.write_all(
                        format!(
                            "HTTP/1.0 200 OK\r\nContent-Type: application/octet-stream\r\n\
                             Content-Disposition: attachment; filename=\"test.bin\"\r\nContent-Length: {}\r\n\r\n",
                            fuer_server.len()
                        )
                        .as_bytes(),
                    );
                    let _ = s.write_all(&fuer_server);
                } else {
                    let html = "<html><body style='margin:0'><a href='/datei' style='display:block;width:400px;height:150px;background:#ff0000'>x</a>\
                                <a href='/kaputt' style='display:block;width:400px;height:150px;background:#0000ff'>y</a></body></html>";
                    let _ = s.write_all(format!("HTTP/1.0 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{html}", html.len()).as_bytes());
                }
            }
        });
        // Link zweimal antippen: test.bin und test (2).bin; dann der Link auf
        // die Datei, deren Übertragung abbricht (B-Review: halbe Datei weg)
        assert!(c_umgebung("NS_TEST_EINGABE", "1200:100:100;2500:100:100;3800:100:250"));
        let (r, _bild) = lauf_mit(&format!("http://127.0.0.1:{port}/"), 800, 480, 5500, false).expect("Bild");
        assert_eq!(r, 0);
        let dl = ordner.join("Downloads");
        let namen: Vec<String> = std::fs::read_dir(&dl).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
        println!("Downloads: {namen:?}");
        assert_eq!(std::fs::read(dl.join("test.bin")).ok().as_deref(), Some(&inhalt[..]), "test.bin unverändert");
        assert_eq!(std::fs::read(dl.join("test (2).bin")).ok().as_deref(), Some(&inhalt[..]), "zweiter Download daneben");
        // SAFETY: Zähler aus c/download.c, NetSurf läuft nicht mehr.
        assert_eq!(unsafe { ns_downloads_fertig() }, 2);
        assert!(!dl.join("kaputt.bin").exists(), "abgebrochener Download gelöscht: {namen:?}");
        let _ = std::fs::remove_dir_all(&ordner);
    }

    /// B8: Zeichen der Bildschirmtastatur kommen als Unicode an – auch
    /// Grossbuchstaben, Umlaute, „%“ und „{}“ (nsfb hat dafür keine Codes).
    /// Adressleiste: „…/MÄ“, zweimal Rücktaste (ganze UTF-8-Zeichen), „X“
    /// → Abruf von /X; im Formular „Ä%M{x}“ → genau diese Zeichen.
    #[test]
    fn tastatur_sonderzeichen() {
        c(tastatur_sonderzeichen_innen);
    }

    fn tastatur_sonderzeichen_innen() {
        if im_eigenen_prozess("tastatur_sonderzeichen") {
            return;
        }
        use std::io::{Read, Write};
        ressourcen();
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_TEST_EINGABE", "");
            }
        }
        let _z = Zuruecksetzen;
        let server = std::net::TcpListener::bind("127.0.0.1:0").expect("Prüfserver");
        let port = server.local_addr().unwrap().port();
        let anfragen = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let log = anfragen.clone();
        std::thread::spawn(move || {
            for s in server.incoming().flatten() {
                let mut s = s;
                let mut kopf = Vec::new();
                let mut b = [0u8; 1];
                while !kopf.ends_with(b"\r\n\r\n") && s.read(&mut b).map(|n| n == 1).unwrap_or(false) {
                    kopf.push(b[0]);
                }
                let k = String::from_utf8_lossy(&kopf).to_string();
                let pfad = k.split_whitespace().nth(1).unwrap_or("/").to_string();
                log.lock().unwrap().push(pfad);
                let html = "<html><body style='margin:0;background:white'><form action='/suche' method='get'>\
                            <input name='q' style='width:300px;height:40px;font-size:24px'></form></body></html>";
                let _ = s.write_all(format!("HTTP/1.0 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{html}", html.len()).as_bytes());
            }
        });
        let eingabe = format!("1000:450:15;1300:T:http://127.0.0.1:{port}/M\\u00C4\\b\\bX\\n;2600:100:50;2900:T:\\u00C4%M{{x}}\\n");
        assert!(c_umgebung("NS_TEST_EINGABE", &eingabe));
        let (r, _bild) = lauf_mit(&format!("http://127.0.0.1:{port}/"), 800, 480, 4000, false).expect("Bild");
        assert_eq!(r, 0);
        let a = anfragen.lock().unwrap().clone();
        println!("Anfragen {a:?}");
        assert!(a.iter().any(|p| p == "/X"), "Adressleiste: /MÄ minus zwei Zeichen plus X: {a:?}");
        let suche = a.iter().find(|p| p.starts_with("/suche?q=")).expect("Formular abgeschickt");
        let q = &suche["/suche?q=".len()..];
        // Prozent-Kodierung auflösen (Formular: UTF-8)
        let mut bytes = Vec::new();
        let qb = q.as_bytes();
        let mut i = 0;
        while i < qb.len() {
            if qb[i] == b'%' && i + 2 < qb.len() {
                bytes.push(u8::from_str_radix(&q[i + 1..i + 3], 16).unwrap());
                i += 3;
            } else {
                bytes.push(if qb[i] == b'+' { b' ' } else { qb[i] });
                i += 1;
            }
        }
        assert_eq!(String::from_utf8_lossy(&bytes), "Ä%M{x}", "Formular: {suche}");
    }

    /// B10: Ein HTML-Dokument über der Grenze (am Board 512 KB, hier per
    /// NS_HTML_MAX 20 000 Bytes) wird abgeschnitten: der Anfang erscheint,
    /// das Ende nicht, der Browser läuft weiter.
    #[test]
    fn html_abgeschnitten() {
        c(html_abgeschnitten_innen);
    }

    fn html_abgeschnitten_innen() {
        if im_eigenen_prozess("html_abgeschnitten") {
            return;
        }
        use std::io::{Read, Write};
        unsafe extern "C" {
            fn ns_abruf_gekuerzt() -> u32;
        }
        ressourcen();
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_HTML_MAX", "");
            }
        }
        let _z = Zuruecksetzen;
        assert!(c_umgebung("NS_HTML_MAX", "20000"));
        let server = std::net::TcpListener::bind("127.0.0.1:0").expect("Prüfserver");
        let port = server.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in server.incoming().flatten() {
                let mut s = s;
                let mut kopf = Vec::new();
                let mut b = [0u8; 1];
                while !kopf.ends_with(b"\r\n\r\n") && s.read(&mut b).map(|n| n == 1).unwrap_or(false) {
                    kopf.push(b[0]);
                }
                let k = String::from_utf8_lossy(&kopf).to_string();
                if k.split_whitespace().nth(1) != Some("/") {
                    let _ = s.write_all(b"HTTP/1.0 404 Not Found\r\nContent-Length: 0\r\n\r\n");
                    continue;
                }
                let mut html = String::from(
                    "<html><body style='margin:0;background:white'>\
                     <div style='width:300px;height:100px;background:#ff0000'></div>",
                );
                while html.len() < 60_000 {
                    html.push_str("<!-- Fuelltext, damit die Seite gross wird ------------------------------ -->\n");
                }
                html.push_str("<div style='width:300px;height:100px;background:#00ff00'></div></body></html>");
                let _ = s.write_all(
                    format!("HTTP/1.0 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{html}", html.len())
                        .as_bytes(),
                );
            }
        });
        let (r, bild) = lauf_mit(&format!("http://127.0.0.1:{port}/"), 640, 360, 2500, false).expect("Bild");
        assert_eq!(r, 0);
        let rot = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0xFF_0000).count();
        let gruen = bild.px.iter().filter(|&&p| p & 0xFF_FFFF == 0x00_FF00).count();
        assert!(rot >= 300 * 100, "Anfang der Seite sichtbar ({rot} px)");
        assert_eq!(gruen, 0, "Ende der Seite abgeschnitten");
        // SAFETY: Zähler aus c/abruf.c, NetSurf läuft nicht mehr.
        assert_eq!(unsafe { ns_abruf_gekuerzt() }, 1);
    }

    /// B-Review: Tab schliessen, während er noch lädt („X“ in der
    /// Tab-Liste). Danach Seite antippen und tippen (Eingabefokus der
    /// Wurzel zeigte sonst auf das zerstörte Fenster) und weiterlaufen lassen
    /// (der Throbber des geschlossenen Tabs lief sonst weiter) – der Browser
    /// endet normal, der erste Tab ist zu sehen.
    #[test]
    fn tab_schliessen_beim_laden() {
        c(tab_schliessen_beim_laden_innen);
    }

    fn tab_schliessen_beim_laden_innen() {
        if im_eigenen_prozess("tab_schliessen_beim_laden") {
            return;
        }
        use std::io::{Read, Write};
        unsafe extern "C" {
            fn ns_tabs_hoechststand() -> i32;
        }
        ressourcen();
        struct Zuruecksetzen;
        impl Drop for Zuruecksetzen {
            fn drop(&mut self) {
                c_umgebung("NS_TEST_EINGABE", "");
            }
        }
        let _z = Zuruecksetzen;
        let server = std::net::TcpListener::bind("127.0.0.1:0").expect("Prüfserver");
        let port = server.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in server.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut s = s;
                    let mut kopf = Vec::new();
                    let mut b = [0u8; 1];
                    while !kopf.ends_with(b"\r\n\r\n") && s.read(&mut b).map(|n| n == 1).unwrap_or(false) {
                        kopf.push(b[0]);
                    }
                    let k = String::from_utf8_lossy(&kopf).to_string();
                    let pfad = k.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let html = if pfad == "/langsam" {
                        // lädt noch, wenn der Tab geschlossen wird
                        std::thread::sleep(std::time::Duration::from_millis(3000));
                        "<html><body style='margin:0;background:#0000ff'></body></html>"
                    } else if pfad == "/" {
                        "<html><body style='margin:0;background:#00ff00'>\
                         <a href='/langsam' target='_blank' style='display:block;width:400px;height:200px;background:#ff0000'>x</a>\
                         <form action='/' method='get'><input name='q' style='width:300px;height:40px'></form></body></html>"
                    } else {
                        ""
                    };
                    let _ = s.write_all(format!("HTTP/1.0 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{html}", html.len()).as_bytes());
                });
            }
        });
        let farbe = |bild: &Bild, x: usize, y: usize| bild.px[y * bild.breite as usize + x] & 0xFF_FFFF;
        // Link (neuer Tab lädt 3 s), „Tabs 2“, „X“ der ersten Zeile (der neue
        // Tab), dann Formularfeld antippen und tippen, weiterlaufen lassen
        assert!(c_umgebung("NS_TEST_EINGABE", "1200:100:100;1800:770:15;2400:754:83;3200:100:255;3500:T:abc"));
        let (r, bild) = lauf_mit(&format!("http://127.0.0.1:{port}/"), 800, 480, 6000, false).expect("Bild");
        assert_eq!(r, 0, "Browser endet normal");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
        std::fs::write(dir.join("b8_pc_tab_geschlossen.png"), ohne_adresse(&bild)).unwrap();
        // SAFETY: Zähler aus c/tabs.c, NetSurf läuft nicht mehr.
        assert_eq!(unsafe { ns_tabs_hoechststand() }, 2, "zwei Tabs offen gewesen");
        assert_eq!(farbe(&bild, 600, 150), 0x00_FF00, "erster Tab zu sehen");
        assert_eq!(farbe(&bild, 100, 100), 0xFF_0000, "Link des ersten Tabs");
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
        // „null“: nicht einmal das Fehlerobjekt passte noch (i686 mit der
        // Reserve des Programm-Heaps, B10) – ebenfalls die Speichergrenze
        assert!(e.contains("out of memory") || e.contains("memory") || e == "null", "{e}");
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

