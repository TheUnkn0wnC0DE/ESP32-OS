//! Q2: Wetter-App per Touch – Ortssuche über die Bildschirmtastatur (Taste
//! für Taste), Treffer antippen, Wetter und Vorschau erscheinen, Ort bleibt
//! gespeichert; ohne Netz eine verständliche Meldung. Der Abrufer liefert
//! feste Open-Meteo-Antworten (kein Netz im Test).
use desktop::gesten::Gesten;
use desktop::tastatur::{HOEHE, sondertaste, tastenmitte};
use kern::{KernAlloc, sys};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

static NETZ: AtomicBool = AtomicBool::new(true);

struct Fest;
impl kern::abruf::Abrufer for Fest {
    fn text(&self, url: &str) -> Result<(i32, String), String> {
        if !NETZ.load(Ordering::SeqCst) {
            return Err("ESP_ERR_HTTP_CONNECT".into());
        }
        if url.contains("geocoding-api.open-meteo.com") {
            assert!(url.contains("name=zuerich"), "Suche: {url}");
            return Ok((200, r#"{"results":[{"name":"Zürich","latitude":47.36667,"longitude":8.55,"country":"Schweiz","admin1":"Zürich"},{"name":"Zurich","latitude":43.42,"longitude":-81.62,"country":"Kanada","admin1":"Ontario"}]}"#.into()));
        }
        if url.contains("api.open-meteo.com/v1/forecast") {
            assert!(url.contains("latitude=47.3667") && url.contains("longitude=8.5500"), "Vorhersage: {url}");
            return Ok((200, r#"{"current":{"temperature_2m":4.6,"relative_humidity_2m":81,"weather_code":61,"wind_speed_10m":12.3},"daily":{"time":["2026-11-25","2026-11-26","2026-11-27","2026-11-28","2026-11-29"],"weather_code":[61,3,0,71,95],"temperature_2m_max":[6.1,5.0,7.9,1.0,9.4],"temperature_2m_min":[1.2,-0.4,-2.0,-3.5,4.0]}}"#.into()));
        }
        Ok((404, String::new()))
    }
}

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(8), "Zeitüberschreitung: {was}; Fenster {:?}", sys::fenster_liste().iter().map(|f| f.titel.clone()).collect::<Vec<_>>());
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn tipp(g: &mut Gesten, t: &mut u64, x: i32, y: i32) {
    g.schritt(*t, [Some((x, y)), None]);
    *t += 100;
    g.schritt(*t, [None, None]);
    for _ in 0..8 {
        *t += 20;
        g.schritt(*t, [None, None]);
        std::thread::sleep(Duration::from_millis(10));
    }
    *t += 400;
    std::thread::sleep(Duration::from_millis(120));
}

fn foto(name: &str) {
    std::thread::sleep(Duration::from_millis(300));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
}

fn titel_da(t: &str) -> bool {
    sys::fenster_liste().iter().any(|f| f.titel == t)
}

#[test]
fn wetter_per_touch() {
    let wurzel = std::env::temp_dir().join(format!("esp32os_q2_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(&wurzel).unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    for p in ["/sys", "/rd/1", "/sd0/1"] {
        kern::datei::einhaengen(p, &wurzel);
    }
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    assert!(kern::abruf::abrufer_setzen(Box::new(Fest)));
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    assert!(desktop::anwendungen().iter().any(|a| a.programm == "wetter" && a.auf_desktop), "Symbol auf dem Desktop");
    for p in ["@taskbar", "@tastatur"] {
        kern::starte_name(p).unwrap();
    }
    std::thread::sleep(Duration::from_millis(500));
    let mut g = Gesten::default();
    let mut t = 1000u64;

    // ohne gespeicherten Ort: gleich die Ortssuche
    let pid = kern::starte_name("wetter").unwrap();
    bis(|| sys::tastatur_soll().0, "Suchfeld mit Tastatur");
    std::thread::sleep(Duration::from_millis(300));
    let oben = match sys::tastatur_soll().1 {
        Some(c) if c.y1 > 360 - desktop::shell::LEISTE_H - HOEHE => (c.y1 + 2).min(360 - HOEHE),
        _ => 360 - desktop::shell::LEISTE_H - HOEHE,
    };
    for c in "zuerich".chars() {
        let (_, x, y) = tastenmitte(c, 640).unwrap();
        tipp(&mut g, &mut t, x, oben + y);
    }
    let (x, y) = sondertaste("enter", 640);
    tipp(&mut g, &mut t, x, oben + y);
    bis(|| titel_da("Weather") && !sys::tastatur_soll().0, "Trefferliste");
    std::thread::sleep(Duration::from_millis(400));
    foto("q2_treffer");
    // erster Treffer (Fenster 20,8 + Arbeitsbereich 5,48; Zeile 32..72)
    tipp(&mut g, &mut t, 20 + 5 + 200, 8 + 48 + 32 + 20);
    std::thread::sleep(Duration::from_millis(600));
    foto("q2_wetter");
    let ort = desktop::konfig::wetter_ort().expect("Ort gespeichert");
    assert_eq!(ort.name, "Zürich, Schweiz");
    kern::zwangsbeenden(pid, 2000);
    bis(|| !titel_da("Weather"), "Wetter zu");

    // Neustart: Ort aus den Einstellungen, ohne Netz → Meldung
    NETZ.store(false, Ordering::SeqCst);
    let pid = kern::starte_name("wetter").unwrap();
    bis(|| titel_da("Weather"), "Wetter offen");
    std::thread::sleep(Duration::from_millis(800));
    foto("q2_ohne_wlan");
    kern::zwangsbeenden(pid, 2000);
    let _ = std::fs::remove_dir_all(&wurzel);
}
