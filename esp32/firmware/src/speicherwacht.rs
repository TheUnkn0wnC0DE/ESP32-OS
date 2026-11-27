//! M1 (Messen statt raten): Speicherwacht. Jede Sekunde den Speicherstand
//! (frei und grösster Block, intern und PSRAM) in den Absturzmerker
//! (`absturz.c`, nach einem Absturz steht er in der Meldung); alle 30 s eine
//! Zeile ins Log mit den Tiefstständen seit dem Start und dem Verbrauch
//! jedes laufenden Programms (dazu einmal bei t=20 s, Ausgangswert für M4). Mess-Build (`ESP32OS_SPEICHERBERICHT=30,600`
//! Sekunden): zu diesen Zeiten der volle Bericht (`speicherbericht.c`).

use esp_idf_svc::sys;
use std::time::Duration;

unsafe extern "C" {
    fn esp32os_speicher_merken(intern: u32, intern_block: u32, psram: u32, psram_block: u32, sekunden: u32);
    fn esp32os_speicherbericht();
}

/// Speicherstand in KB: (intern frei, intern Block, PSRAM frei, PSRAM Block).
pub fn stand() -> (u32, u32, u32, u32) {
    let kb = |caps: u32, block: bool| {
        // SAFETY: reine Abfragen der Heap-Verwaltung.
        (unsafe { if block { sys::heap_caps_get_largest_free_block(caps) } else { sys::heap_caps_get_free_size(caps) } } / 1024) as u32
    };
    (
        kb(sys::MALLOC_CAP_INTERNAL, false),
        kb(sys::MALLOC_CAP_INTERNAL, true),
        kb(sys::MALLOC_CAP_SPIRAM, false),
        kb(sys::MALLOC_CAP_SPIRAM, true),
    )
}

/// Logzeile mit dem Stand und den Programmen (auch für Messläufe).
pub fn zeile(sekunden: u64) -> String {
    let (i, ib, p, pb) = stand();
    // SAFETY: reine Abfragen.
    let (imin, pmin) = unsafe {
        (
            sys::heap_caps_get_minimum_free_size(sys::MALLOC_CAP_INTERNAL) / 1024,
            sys::heap_caps_get_minimum_free_size(sys::MALLOC_CAP_SPIRAM) / 1024,
        )
    };
    let programme: Vec<String> = kern::liste()
        .into_iter()
        .map(|p| format!("{} {}/{} KB (Spitze {})", p.name, p.heap_belegt / 1024, p.heap_groesse / 1024, p.heap_spitze / 1024))
        .collect();
    // M6: Anteil der Fensterpuffer am Kern-System
    let fenster = kern::fenster::fensterpuffer();
    let pool = crate::kern_board::pool_stand().map_or(String::new(), |(f, b, g)| format!(" Pool {f}/{g} KB (Block {b})"));
    format!(
        "SPEICHER t={sekunden}s intern frei {i} KB (Block {ib}, Min {imin}) PSRAM frei {p} KB (Block {pb}, Min {pmin}){pool} Kern-System {} KB (Fenster {} KB in {}) | {}",
        kern::system_belegt() / 1024,
        fenster.0 / 1024,
        fenster.1,
        programme.join(", ")
    )
}

/// Speicherwacht starten (Faden mit Stapel im PSRAM).
pub fn starten() {
    let berichte: Vec<u64> = option_env!("ESP32OS_SPEICHERBERICHT")
        .map(|s| s.split(',').filter_map(|t| t.trim().parse().ok()).collect())
        .unwrap_or_default();
    let ok = crate::update::psram_faden(c"speicherwacht", 6 * 1024, move || {
        let beginn = crate::hw::now_us() as u64;
        let mut n: u64 = 0;
        #[cfg(esp32os_heap_trace)]
        let (mut spur_an, mut spur_aus) = (false, false);
        loop {
            let s = (crate::hw::now_us() as u64 - beginn) / 1_000_000;
            let (i, ib, p, pb) = stand();
            // SAFETY: speichert fünf Zahlen (absturz.c).
            unsafe { esp32os_speicher_merken(i, ib, p, pb, s as u32) };
            // alle 30 s; dazu t=20 s als Ausgangswert für den Dauertest
            // (Desktop steht, WLAN da, Autopilot noch nicht begonnen)
            if n % 30 == 0 || s == 20 {
                log::info!("{}", zeile(s));
            }
            // M4-Lecksuche (Build `ESP32OS_HEAP_TRACE`): Spur ab 60 s (Desktop
            // ruht), Ausgabe nach `ESP32OS_HEAPSPUR_ENDE` s (Ende einer
            // Autopilot-Runde, alle Programme zu) – was dann noch belegt ist
            // und sich wiederholt, ist das Leck
            #[cfg(esp32os_heap_trace)]
            {
                let ende: u64 = option_env!("ESP32OS_HEAPSPUR_ENDE").and_then(|t| t.parse().ok()).unwrap_or(885);
                if s >= 60 && !spur_an && option_env!("ESP32OS_SEITENLISTE").is_none() {
                    spur_an = true;
                    crate::seitenliste::spur::starten();
                }
                if s >= ende && spur_an && !spur_aus {
                    spur_aus = true;
                    crate::seitenliste::spur::ausgeben();
                }
            }
            if berichte.contains(&s) {
                // SAFETY: Bericht über Tasks und Heap (speicherbericht.c).
                unsafe { esp32os_speicherbericht() };
            }
            n += 1;
            std::thread::sleep(Duration::from_secs(1));
        }
    });
    if !ok {
        log::error!("Speicherwacht nicht gestartet");
    }
}

/// M8 (Entwicklungs-Build `ESP32OS_SYSTEM_RESTKB=<KB>`): System-Heap (PSRAM
/// ausserhalb des Programm-Pools) absichtlich bis auf `rest_kb` füllen und
/// so lassen – Nachweis, dass ein fast leerer System-Heap das System nicht
/// abbricht (Programme werden mit Meldung abgelehnt). Belegt in 64-KB-
/// Stücken, kleinere am Ende; schreibt das Ergebnis ins Log.
pub fn system_fuellen(rest_kb: usize) {
    let mut belegt_kb = 0usize;
    let mut stueck = 64 * 1024usize;
    loop {
        // SAFETY: reine Abfrage.
        let frei = unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT) };
        if frei <= rest_kb * 1024 + 1024 {
            break;
        }
        let n = stueck.min(frei - rest_kb * 1024);
        // SAFETY: Belegung im PSRAM; bleibt absichtlich bis zum Neustart.
        let p = unsafe { sys::heap_caps_malloc(n, sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT) };
        if p.is_null() {
            if stueck <= 1024 {
                break;
            }
            stueck /= 2;
            continue;
        }
        belegt_kb += n / 1024;
    }
    // SAFETY: reine Abfrage.
    let frei = unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT) };
    log::warn!("M8-Test: System-Heap gefüllt – {belegt_kb} KB belegt, {} KB frei (Ziel {rest_kb} KB)", frei / 1024);
}
