//! W3-Nachweis (PC): WLAN in den Einstellungen per Touch – Netze suchen
//! (Liste mit Signal, Sicherheit, Wi-Fi 6, Blättern), unsicheres Netz
//! abgelehnt, falsches Passwort, Passwort über die Bildschirmtastatur,
//! verbunden und gemerkt, Vergessen, Trennen; W3b: Internetzeit in
//! „Datum & Zeit“ (Status nach NTP-Abgleich, ein/aus gespeichert); W6:
//! Antippen der WLAN-Anzeige öffnet Einstellungen → WLAN; U1: System-Seite
//! (Update suchen und installieren, Quelle simuliert). Treiber ist
//! `kern::wlan::Simulation`; die Rolle der Firmware (`wache` alle 100 ms)
//! übernimmt ein Faden.

use desktop::einstellungen::{KH, KW, kachel_lage};
use desktop::gesten::Gesten;
use desktop::tastatur::{HOEHE, tastenmitte};
use kern::wlan::{self, Netz, Sicherheit, Simulation};
use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(8), "Zeitüberschreitung: {was}; Fenster: {:?}", sys::fenster_liste().iter().map(|f| f.titel.clone()).collect::<Vec<_>>());
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Finger {
    g: Gesten,
    t: u64,
}

impl Finger {
    fn tippen(&mut self, x: i32, y: i32) {
        let mut d = 0;
        while d <= 80 {
            self.g.schritt(self.t + d, [Some((x, y)), None]);
            d += 50;
            std::thread::sleep(Duration::from_millis(5));
        }
        self.t += 130;
        self.g.schritt(self.t, [None, None]);
        std::thread::sleep(Duration::from_millis(60));
        self.t += 100;
        self.g.schritt(self.t, [None, None]);
        self.t += 600;
        std::thread::sleep(Duration::from_millis(220));
    }
}

fn foto(name: &str) {
    std::thread::sleep(Duration::from_millis(300));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
}

fn da(titel: &str) -> bool {
    sys::fenster_liste().iter().any(|f| f.titel == titel)
}

/// Text über die Bildschirmtastatur tippen (Kleinbuchstaben).
fn tippe(f: &mut Finger, text: &str) {
    bis(|| sys::tastatur_soll().0, "Tastatur erscheint");
    std::thread::sleep(Duration::from_millis(300));
    let y_unten = 360 - desktop::shell::LEISTE_H - HOEHE;
    let oben = match sys::tastatur_soll().1 {
        Some(c) if c.y1 > y_unten && c.y0 >= HOEHE => 0,
        Some(c) if c.y1 > y_unten => (c.y1 + 2).min(360 - HOEHE),
        _ => y_unten,
    };
    for c in text.chars() {
        let (ebene, x, y) = tastenmitte(c, 640).unwrap_or_else(|| panic!("keine Taste für {c}"));
        assert_eq!(ebene, "normal", "{c}");
        f.tippen(x, oben + y);
    }
}

// Einstellungen: Fenster (0, 0), Arbeitsbereich ab (5, 48)
fn kachel(i: usize) -> (i32, i32) {
    let (x, y) = kachel_lage(i);
    (5 + x + KW / 2, 48 + y + KH / 2)
}
/// Netzzeile k (Liste ab y = 58 + 22, Zeilenhöhe 40).
fn zeile(k: i32) -> (i32, i32) {
    (5 + 8 + 200, 48 + 58 + 22 + k * desktop::wlan::ZEILE_H + 17)
}
const VERGESSEN_X: i32 = 5 + 528 + 20;
const TRENNEN: (i32, i32) = (5 + 322 + 73, 48 + 6 + 21);
const RUNTER: (i32, i32) = (5 + 578 + 22, 48 + 80 + 80 + 40);
/// Passwort-Dialog: Fenster bei x = 20, OK bei (380, 3) im Arbeitsbereich.
const OK: (i32, i32) = (20 + 5 + 380 + 45, 48 + 3 + 21);

fn netz(ssid: &str, rssi: i32, s: Sicherheit, ax: bool) -> Netz {
    Netz { ssid: ssid.into(), rssi, kanal: 6, sicherheit: s, ax }
}

#[test]
fn wlan_in_den_einstellungen_per_touch() {
    let wurzel = std::env::temp_dir().join("esp32os_w3_test");
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("settings")).unwrap();
    std::fs::write(wurzel.join("settings/esp32os.ini"), "[system]\nsetup=1\n").unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &wurzel);
    kern::datei::einhaengen("/rd/1", &wurzel);
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());

    // WLAN-Treiber wie am Board, „Firmware“-Faden mit wache()
    let sim = Simulation::neu(vec![
        (netz("Heimnetz", -45, Sicherheit::Wpa2Wpa3, true), "sommerwind".into()),
        (netz("Gast", -60, Sicherheit::Offen, false), String::new()),
        (netz("Nachbar", -71, Sicherheit::Wpa2, false), "x".into()),
        (netz("Altes WEP", -80, Sicherheit::Wep, false), String::new()),
        (netz("Buero", -86, Sicherheit::Wpa3, true), "y".into()),
    ]);
    assert!(wlan::treiber_setzen(Box::new(sim)));
    // Update-Quelle (U1) simuliert: neueste Freigabe v99.0 (immer neuer als VERSION)
    let upd: &'static kern::aktualisierung::Simulation = Box::leak(Box::new(kern::aktualisierung::Simulation {
        neueste: kern::aktualisierung::Freigabe {
            version: "v99.0".into(),
            bild_url: "https://example.org/esp32os.bin".into(),
            pruef_url: "https://example.org/esp32os.bin.sha256".into(),
        },
        installiert: std::sync::Mutex::new(None),
    }));
    UPDATE.set(upd).ok();
    use kern::aktualisierung::Quelle;
    assert!(kern::aktualisierung::quelle_setzen(Box::new(UpdateWeiter(upd))));
    std::thread::spawn(|| {
        let t0 = Instant::now();
        loop {
            wlan::wache(t0.elapsed().as_millis() as u64);
            std::thread::sleep(Duration::from_millis(100));
        }
    });

    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    kern::starte_name("einstellungen").unwrap();
    bis(|| da("Settings"), "Einstellungen offen");
    foto("w3_kacheln");
    let mut f = Finger { g: Gesten::default(), t: 1000 };

    // WLAN-Kachel (Nr. 10): sucht beim Öffnen; noch kein Netz gemerkt
    let (x, y) = kachel(9);
    f.tippen(x, y);
    std::thread::sleep(Duration::from_millis(400));
    foto("w3_liste");
    assert!(wlan::bekannte().is_empty());

    // Unsicheres Netz (WEP, Zeile 3): abgelehnt, kein Passwort-Dialog
    let (x, y) = zeile(3);
    f.tippen(x, y);
    std::thread::sleep(Duration::from_millis(300));
    assert!(!da("Password for Altes WEP"), "WEP wird nicht angeboten");
    foto("w3_wep");

    // Falsches Passwort: nicht verbunden, Hinweis, nicht gemerkt
    let (x, y) = zeile(0);
    f.tippen(x, y);
    bis(|| da("Password for Heimnetz"), "Passwort-Dialog");
    tippe(&mut f, "falsch");
    f.tippen(OK.0, OK.1);
    bis(|| !da("Password for Heimnetz"), "Dialog zu");
    bis(|| wlan::zustand().grund == 15 && !wlan::zustand().verbindet, "Verbindung abgelehnt");
    std::thread::sleep(Duration::from_millis(300));
    foto("w3_falsch");
    assert!(!wlan::zustand().verbunden);
    assert!(wlan::bekannte().is_empty(), "falsches Passwort nicht gemerkt");

    // Richtiges Passwort über die Bildschirmtastatur → verbunden, gemerkt
    f.tippen(x, y);
    bis(|| da("Password for Heimnetz"), "Passwort-Dialog 2");
    tippe(&mut f, "sommerwind");
    f.tippen(OK.0, OK.1);
    bis(|| wlan::zustand().verbunden, "verbunden");
    bis(|| wlan::bekannte() == ["Heimnetz"], "gemerkt");
    assert_eq!(wlan::verbunden_mit().as_deref(), Some("Heimnetz"));
    assert!(wlan::zustand().he, "802.11ax");
    std::thread::sleep(Duration::from_millis(600));
    foto("w3_verbunden");

    // Blättern: fünftes Netz erscheint
    f.tippen(RUNTER.0, RUNTER.1);
    std::thread::sleep(Duration::from_millis(300));
    foto("w3_geblaettert");
    f.tippen(RUNTER.0 , 48 + 80 + 20);

    // Trennen von Hand, dann gemerktes Netz ohne Dialog wieder verbinden
    f.tippen(TRENNEN.0, TRENNEN.1);
    bis(|| !wlan::zustand().verbunden, "getrennt");
    std::thread::sleep(Duration::from_millis(300));
    let (x, y) = zeile(0);
    f.tippen(x, y);
    std::thread::sleep(Duration::from_millis(200));
    assert!(!da("Password for Heimnetz"), "gemerktes Netz braucht kein Passwort");
    bis(|| wlan::zustand().verbunden, "wieder verbunden");

    // Vergessen („x“ in der Zeile)
    f.tippen(VERGESSEN_X, y);
    bis(|| wlan::bekannte().is_empty(), "vergessen");
    std::thread::sleep(Duration::from_millis(300));
    foto("w3_vergessen");

    // W3b: Die Plattform meldet einen NTP-Abgleich; „Datum & Zeit“ zeigt ihn,
    // der Schalter „Internetzeit“ wird gespeichert
    assert!(kern::zeit::ntp_an(), "Standard: Internetzeit an");
    kern::zeit::ntp_gemeldet();
    assert!(kern::zeit::ntp_gestellt_um().is_some());
    f.tippen(5 + 8 + 28, 48 + 6 + 21);
    let (x, y) = kachel(3);
    f.tippen(x, y);
    std::thread::sleep(Duration::from_millis(400));
    foto("w3b_zeit");
    let ntp = (5 + 246 + 187, 48 + 204 + 22);
    let ini = || std::fs::read_to_string(wurzel.join("settings/esp32os.ini")).unwrap_or_default();
    f.tippen(ntp.0, ntp.1);
    bis(|| !kern::zeit::ntp_an(), "Internetzeit aus");
    assert!(ini().contains("ntp=0"), "{}", ini());
    foto("w3b_zeit_aus");
    f.tippen(ntp.0, ntp.1);
    bis(|| kern::zeit::ntp_an(), "Internetzeit wieder an");
    assert!(ini().contains("ntp=1"), "{}", ini());

    // U1: System-Seite – neuere Version gefunden, installiert (simuliert)
    f.tippen(5 + 8 + 28, 48 + 6 + 21);
    let (x, y) = kachel(10);
    f.tippen(x, y);
    std::thread::sleep(Duration::from_millis(300));
    f.tippen(5 + 20 + 150, 48 + 116 + 24);
    std::thread::sleep(Duration::from_millis(500));
    foto("u1_gefunden");
    f.tippen(5 + 330 + 145, 48 + 116 + 24);
    bis(|| UPDATE.get().is_some_and(|u| u.installiert.lock().unwrap().as_deref() == Some("v99.0")), "Update installiert");
    std::thread::sleep(Duration::from_millis(300));
    foto("u1_installiert");

    // W6: Antippen der WLAN-Anzeige in der Taskleiste → Einstellungen, WLAN
    let anzahl = || sys::fenster_liste().iter().filter(|f| f.titel == "Settings").count();
    assert_eq!(anzahl(), 1);
    f.tippen(640 - 148 + 2 + 16, 360 - 48 + 3 + 21);
    bis(|| anzahl() == 2, "Einstellungen über die WLAN-Anzeige geöffnet");
    std::thread::sleep(Duration::from_millis(500));
    foto("w6_wlan_aus_leiste");
}

static UPDATE: std::sync::OnceLock<&'static kern::aktualisierung::Simulation> = std::sync::OnceLock::new();

/// Reicht an die Simulation weiter (bleibt für die Prüfung greifbar).
struct UpdateWeiter(&'static kern::aktualisierung::Simulation);
impl kern::aktualisierung::Quelle for UpdateWeiter {
    fn neueste(&self) -> Result<kern::aktualisierung::Freigabe, String> {
        self.0.neueste()
    }
    fn installieren(&self, f: &kern::aktualisierung::Freigabe, p: &mut dyn FnMut(u32)) -> Result<(), String> {
        self.0.installieren(f, p)
    }
}
