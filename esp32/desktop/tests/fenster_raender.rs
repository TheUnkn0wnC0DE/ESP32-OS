//! S5: Jedes Programm öffnen und prüfen, dass Titelleiste, Ränder und Ecken
//! seines Fensters vollständig und gleich gezeichnet sind – genau so, wie
//! der Kern Rahmen und Kopf neu zeichnet (kein Programminhalt darüber) –
//! und dass das Fenster ganz auf dem Bildschirm liegt. Fotos je Programm in
//! `tests/erwartet/s5/` (`S5_FOTOS=vorher` bzw. ohne: `nachher`).
//!
//! Fehlerbild (Nutzer 24.11.): KolibriOS-Programme ohne Flag „relativ“
//! (PasswordGen, TinyHashView, Beispiel-GUI) zeichneten ihren Inhalt in die
//! höhere Touch-Titelleiste – der obere Rand wirkte abgehackt.
#![cfg(target_pointer_width = "32")]
use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

/// Ohne Fenster, Testprogramme, Dienste und Spiele.
const AUSSEN_VOR: &[&str] = &["absturz_test", "endlostest", "messung", "http_tcp_demo", "netz_laden"];

/// Fensterbild ruhig? (zweimal gleich im Abstand von 150 ms)
fn ruhig(slot: usize) -> Option<Vec<u16>> {
    let a = kern::fenster::mit_welt(|w| w.slots.get(slot)?.as_ref()?.fenster.as_ref().map(|f| f.buf.px.clone()))?;
    std::thread::sleep(Duration::from_millis(150));
    let b = kern::fenster::mit_welt(|w| w.slots.get(slot)?.as_ref()?.fenster.as_ref().map(|f| f.buf.px.clone()))?;
    (a == b).then_some(a)
}

/// Abweichungen von Titelleiste und Rahmen gegenüber der Neuzeichnung des
/// Kerns (Beschreibung je Fehler).
fn rahmen_pruefen(slot: usize) -> Vec<String> {
    kern::fenster::mit_welt(|w| {
        let skin = w.skin.clone();
        let titel_farbe = w.farben[4];
        let aktiv = w.aktiv() == Some(slot);
        let (sw, sh) = (w.w, w.h);
        let Some(f) = w.slots.get_mut(slot).and_then(|s| s.as_mut()).and_then(|s| s.fenster.as_mut()) else {
            return vec!["kein Fenster".into()];
        };
        let mut fehler = Vec::new();
        if f.x < 0 || f.y < 0 || f.x + f.w > sw || f.y + f.h > sh {
            fehler.push(format!("nicht ganz auf dem Bildschirm: {}×{} bei ({}, {})", f.w, f.h, f.x, f.y));
        }
        if !matches!(f.stil, 3 | 4) {
            return fehler;
        }
        let ist = f.buf.clone();
        kern::fenster::rahmen_zeichnen(f, &skin, aktiv);
        kern::fenster::kopf_zeichnen(f, &skin, aktiv, titel_farbe);
        let soll = std::mem::replace(&mut f.buf, ist);
        let (bw, bh, k, r) = (f.w as usize, f.h as usize, skin.hoehe as usize, kern::fenster::RAHMEN as usize);
        let mut zaehlen = |name: &str, x0: usize, y0: usize, x1: usize, y1: usize| {
            let mut n = 0;
            let mut erst = None;
            for y in y0..y1.min(bh) {
                for x in x0..x1.min(bw) {
                    if f.buf.px[y * bw + x] != soll.px[y * bw + x] {
                        n += 1;
                        erst.get_or_insert((x, y));
                    }
                }
            }
            if n > 0 {
                fehler.push(format!("{name}: {n} Bildpunkte anders, erster bei {:?}", erst.unwrap_or_default()));
            }
        };
        zaehlen("Titelleiste", 0, 0, bw, k);
        zaehlen("linker Rand", 0, k, r, bh);
        zaehlen("rechter Rand", bw.saturating_sub(r), k, bw, bh);
        zaehlen("unterer Rand", 0, bh.saturating_sub(r), bw, bh);
        fehler
    })
}

#[test]
fn alle_fenster_mit_sauberem_rahmen() {
    let wurzel = std::env::temp_dir().join(format!("esp32os_s5_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(&wurzel).unwrap();
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../kolibri/daten/logo.png"), wurzel.join("logo.png")).unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    for p in ["/sys", "/rd/1", "/sd0/1"] {
        kern::datei::einhaengen(p, &wurzel);
    }
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    for p in ["@icon", "@taskbar"] {
        kern::starte_name(p).unwrap();
    }
    std::thread::sleep(Duration::from_millis(1500));
    let welche = std::env::var("S5_FOTOS").unwrap_or_else(|_| "nachher".into());
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet/s5").join(&welche);
    std::fs::create_dir_all(&dir).unwrap();
    let mut alle_fehler = Vec::new();
    let mut geprueft = 0;
    // Dialog-Demo zuletzt: ihr Farbdialog bleibt nach dem Kreuz offen
    let mut liste: Vec<&'static kern::ProgrammEintrag> = desktop::PROGRAMME.iter().collect();
    liste.sort_by_key(|e| e.name == "dialog_demo");
    for e in liste {
        let name = e.name;
        if name.starts_with('@') || AUSSEN_VOR.contains(&name) {
            continue;
        }
        let vorher: Vec<usize> = sys::fenster_liste().iter().map(|f| f.slot).collect();
        // TinyHashView öffnet ohne Datei nur den Dateidialog – mit Datei
        // das eigene Fenster (Stil 0x14, ohne „relativ“)
        let start = if name == "thashview" { kern::starte_mit(e, "/sys/logo.png") } else { kern::starte_name(name) };
        let Ok(pid) = start else {
            alle_fehler.push(format!("{name}: startet nicht"));
            continue;
        };
        // Fenster des Programms (Browser braucht länger)
        let t = Instant::now();
        let grenze = Duration::from_secs(if name == "browser" { 20 } else { 6 });
        let mut slot = None;
        while t.elapsed() < grenze {
            if let Some(f) = sys::fenster_liste().into_iter().find(|f| f.pid == pid && !vorher.contains(&f.slot)) {
                slot = Some(f.slot);
                if ruhig(f.slot).is_some() {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let Some(slot) = slot else {
            alle_fehler.push(format!("{name}: kein Fenster"));
            kern::zwangsbeenden(pid, 2000);
            continue;
        };
        std::thread::sleep(Duration::from_millis(300));
        let (w, h, px) = sys::foto();
        desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
        for f in rahmen_pruefen(slot) {
            alle_fehler.push(format!("{name}: {f}"));
        }
        geprueft += 1;
        // über das Kreuz schliessen (Zwangsbeenden wickelt am PC durch
        // C-Rahmen ab – Dialoge der C-Programme brächen den Test ab)
        for _ in 0..6 {
            let Some(s) = sys::fenster_liste().into_iter().filter(|f| !vorher.contains(&f.slot) && !f.titel.starts_with('@')).map(|f| f.slot).last() else { break };
            let kreuz = kern::fenster::mit_welt(|w| {
                let f = w.slots.get(s)?.as_ref()?.fenster.as_ref()?;
                let k = f.knoepfe.iter().find(|k| k.system && k.id == 1)?;
                Some((f.x + k.x + k.w / 2, f.y + k.y + k.h / 2))
            });
            let Some((x, y)) = kreuz else { break };
            sys::eingabe::maus(x, y, 1);
            std::thread::sleep(Duration::from_millis(40));
            sys::eingabe::maus(x, y, 0);
            let t = Instant::now();
            while sys::fenster_liste().iter().any(|f| f.slot == s) && t.elapsed() < Duration::from_secs(3) {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        if sys::fenster_liste().iter().any(|f| f.pid == pid) {
            alle_fehler.push(format!("{name}: schliesst nicht über das Kreuz"));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    eprintln!("S5: {geprueft} Programme geprüft, Fotos in {}", dir.display());
    for f in &alle_fehler {
        eprintln!("  {f}");
    }
    let _ = std::fs::remove_dir_all(&wurzel);
    assert!(geprueft >= 20, "zu wenige Programme geprüft: {geprueft}");
    assert!(alle_fehler.is_empty(), "{} Fehler:\n{}", alle_fehler.len(), alle_fehler.join("\n"));
}
