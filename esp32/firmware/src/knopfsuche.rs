//! Entwicklungs-Build `ESP32OS_KNOPFSUCHE=1` (P5): Welche GPIOs wechseln,
//! wenn BOOT oder PWR gedrückt wird? Beobachtet alle unkritischen GPIOs nur
//! lesend (`gpio_input_enable` ändert keine Ausgabe), merkt sich jeden
//! Pegelwechsel und wiederholt die Liste alle 5 s im Log – so lässt sie sich
//! auch später ohne Reset abholen. Pins mit Dauerwechseln (Takte) werden
//! nach 20 Wechseln ausgeblendet.

use esp_idf_svc::sys;

/// Ausgenommen: USB (24, 25), I²C (7, 8), I²S (9, 10, 12, 13), Hintergrund-
/// licht-PWM (26), SD-Karte (39–44).
const AUSGENOMMEN: [i32; 15] = [7, 8, 9, 10, 12, 13, 24, 25, 26, 39, 40, 41, 42, 43, 44];

pub fn starten() {
    std::thread::Builder::new()
        .name("knopfsuche".into())
        .stack_size(8192)
        .spawn(lauf)
        .map(|_| ())
        .unwrap_or_else(|e| log::error!("Knopfsuche: {e}"));
}

fn lauf() {
    let pins: Vec<i32> = (0..=54).filter(|p| !AUSGENOMMEN.contains(p)).collect();
    // SAFETY: nur Eingangspuffer einschalten und lesen.
    let lesen = |p: i32| unsafe { sys::gpio_get_level(p) };
    for &p in &pins {
        // SAFETY: gültige GPIO-Nummer; schaltet nur den Eingang zu.
        unsafe { sys::gpio_input_enable(p) };
    }
    let mut stand: Vec<i32> = pins.iter().map(|&p| lesen(p)).collect();
    let mut zahl = vec![0u32; pins.len()];
    let mut ereignisse: Vec<String> = Vec::new();
    let grund: Vec<String> = pins.iter().zip(&stand).map(|(p, s)| format!("{p}={s}")).collect();
    log::warn!("KNOPFSUCHE: Grundstand {}", grund.join(" "));
    let t0 = std::time::Instant::now();
    let mut zuletzt = 0u64;
    loop {
        for (i, &p) in pins.iter().enumerate() {
            let l = lesen(p);
            if l != stand[i] {
                stand[i] = l;
                zahl[i] += 1;
                if zahl[i] <= 20 && ereignisse.len() < 200 {
                    let ms = t0.elapsed().as_millis();
                    log::warn!("KNOPFSUCHE: GPIO {p} -> {l} bei {ms} ms");
                    ereignisse.push(format!("{p}->{l}@{}s", ms / 1000));
                }
            }
        }
        let s = t0.elapsed().as_secs();
        if s >= zuletzt + 5 {
            zuletzt = s;
            let laut: Vec<String> = pins.iter().zip(&zahl).filter(|(_, z)| **z > 20).map(|(p, _)| p.to_string()).collect();
            log::warn!("KNOPFSUCHE: {} Wechsel bisher: {} | Dauerwechsel (ausgeblendet): {}", ereignisse.len(), ereignisse.join(" "), laut.join(","));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
