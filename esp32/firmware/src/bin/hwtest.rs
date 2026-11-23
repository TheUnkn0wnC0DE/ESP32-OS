//! Hardware-Test für das Waveshare ESP32-P4-WIFI6-Touch-LCD-5.
//!
//! `cargo run --release --bin hwtest`
//!
//! * Schaltet das Display ein und zeigt ein Testbild in 640×360 (bzw.
//!   320×180), gerendert in Streifen auf beiden Kernen, per PPA vergrößert
//!   und gedreht – genau der Weg, den auch das Spiel nimmt.
//! * **Messung beim Start:** je ~3 s 640×360 scharf/weich und 320×180
//!   scharf/weich; danach eine Tabelle im seriellen Monitor. Getrennt
//!   gemessen: Rendern, scharfes Vergrößern (CPU), PPA-Ausgabe, Arbeitszeit
//!   pro Bild ohne Warten auf VSync – gegen das Budget von ~18,2 ms (55 Hz).
//! * Gibt Touch-Koordinaten aus (Bildschirm und serieller Monitor), dazu
//!   Knöpfe, den Pegel von GPIO 35 (BOOT?), Chip-Revision, CPU-Takt,
//!   Speicher, gemessene Bildwiederholrate und – falls eingerichtet – die
//!   Akkuspannung.
//! * Unten drei Tippfelder: Auflösung, Vergrößerung (scharf/weich), Drehung.

use core::fmt::Write as _;
use pudding_core::gfx::{H, MAX_H, MAX_W, W, hex};
use pudding_core::input::{MAX_TOUCH, btn, panel_to_game};
use pudding_core::render::{Scratch, Target};
use pudding_core::testbild::{self, Line, TestInfo, Zone};
use pudding_fw::board::{
    BATTERY, BOOT_CANDIDATE, BUTTONS_ENABLED, FRAME_BUDGET_MS, I2C_PORT, I2C_SCL, I2C_SDA, LCD_H, LCD_W, TOUCH_INT,
    TOUCH_INT_CANDIDATE,
};
use pudding_fw::display::{Display, refreshes};
use pudding_fw::pipeline::{Frame, FrameStats, Pipeline, StripSource};
use pudding_fw::{battery, hw, i2c, touch};

/// Das Testbild als Streifenquelle für die Pipeline.
struct Picture(TestInfo);

impl StripSource for Picture {
    fn render_strip(&self, _scratch: &mut Scratch, t: &mut Target) {
        testbild::draw(&self.0, t);
    }
}

fn line(args: core::fmt::Arguments) -> Line {
    let mut l = Line::new();
    let _ = l.write_fmt(args);
    l
}

/// Die vier Betriebsarten der Startmessung: (640×360?, weich?).
const MODES: [(bool, bool); 4] = [(true, false), (true, true), (false, false), (false, true)];
/// Bilder pro Betriebsart: erst einschwingen, dann messen (~3 s bei 55 Hz).
const WARMUP: u32 = 30;
const MEASURE: u32 = 150;

/// Summen über die gemessenen Bilder einer Betriebsart.
#[derive(Clone, Copy, Default)]
struct Sum {
    n: u32,
    render_us: u64,
    scale_us: u64,
    output_us: u64,
    work_us: u64,
    max_work_us: u32,
}

impl Sum {
    fn add(&mut self, s: &FrameStats, work_us: u32) {
        let max = |f: fn(&pudding_fw::pipeline::CoreStats) -> u32| s.cores.iter().map(f).max().unwrap_or(0) as u64;
        self.n += 1;
        self.render_us += max(|c| c.render_us);
        self.scale_us += max(|c| c.scale_us);
        self.output_us += max(|c| c.output_us);
        self.work_us += work_us as u64;
        self.max_work_us = self.max_work_us.max(work_us);
    }

    fn ms(&self, v: u64) -> f32 {
        v as f32 / self.n.max(1) as f32 / 1000.0
    }
}

fn mode_name(hires: bool, smooth: bool) -> &'static str {
    match (hires, smooth) {
        (true, false) => "640x360 scharf",
        (true, true) => "640x360 weich ",
        (false, false) => "320x180 scharf",
        (false, true) => "320x180 weich ",
    }
}

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("=== Pudding-Panik Hardwaretest ===");
    pudding_fw::log_chip();
    let (rev, _cores, mhz, psram_mb) = hw::chip();

    if let Err(e) = pudding_fw::init_buttons() {
        log::error!("Knöpfe: {e}");
    }
    // Nur zum Herausfinden, ob der BOOT-Knopf an GPIO 35 hängt.
    let _ = hw::input_pullup(&[BOOT_CANDIDATE]);

    let bus = i2c::Bus::new(I2C_PORT, I2C_SDA, I2C_SCL).expect("I2C-Bus");
    let touch = match touch::Touch::new(&bus) {
        Ok(t) => {
            log::info!(
                "Touch GT911 gefunden unter 0x{:02X}, {}",
                t.addr,
                if t.interrupt { "Interrupt" } else { "Abfrage" }
            );
            // Ohne Interrupt: beobachten, ob GPIO 2 beim Berühren wechselt
            // (dann ist der Interrupt-Widerstand bestückt).
            let watch = TOUCH_INT.is_none().then_some(TOUCH_INT_CANDIDATE);
            Some(touch::TouchReader::start(t, watch))
        }
        Err(e) => {
            log::error!("Touch GT911 nicht gefunden: {e}");
            None
        }
    };
    let mut battery = match BATTERY {
        Some(b) => battery::Battery::new(&b).inspect_err(|e| log::error!("Akku-ADC: {e}")).ok(),
        None => {
            log::info!("Akkuanzeige aus (board.rs: BATTERY = None)");
            None
        }
    };

    let mut disp = Display::new(hex(0x000000)).expect("Display");
    disp.set_brightness(8);
    let mut pipeline = Pipeline::new().expect("PPA");
    log::info!("Display und PPA bereit. Messung der vier Betriebsarten läuft (~12 s) …");

    let mut pic = Picture(TestInfo::default());
    let mut prev_touch = [None; MAX_TOUCH];
    let (mut prev_buttons, mut prev_boot) = (0u8, false);
    let mut stats = FrameStats::default();
    let (mut work_us, mut hz) = (0u32, 0.0f32);
    let mut battery_text = Line::new();
    let (mut sec_start, mut sec_frames, mut sec_refresh, mut fps) = (hw::now_us(), 0u32, refreshes(), 0.0f32);
    // Touch: Abfragen pro Sekunde und Pegelwechsel an GPIO 2 pro Sekunde.
    let (mut touch_rate, mut int_changes) = (0u32, 0u32);
    let mut touch_counters = (0u32, 0u32);
    if !BUTTONS_ENABLED {
        log::info!("Knöpfe an der Leiste aus (board.rs: BUTTONS_ENABLED = false)");
    }
    // Startmessung: aktuelle Betriebsart, Bild darin, Ergebnisse.
    let mut bench: Option<(usize, u32)> = Some((0, 0));
    let mut results = [Sum::default(); 4];

    loop {
        let start = hw::now_us();
        let info = &mut pic.0;

        // --- Startmessung ---------------------------------------------------------
        if let Some((i, n)) = bench {
            (info.hires, info.smooth) = MODES[i];
            if n >= WARMUP {
                results[i].add(&stats, work_us);
            }
            bench = if n + 1 < WARMUP + MEASURE {
                Some((i, n + 1))
            } else if i + 1 < MODES.len() {
                Some((i + 1, 0))
            } else {
                log::info!("=== Messung (Mittel über {MEASURE} Bilder, Budget {FRAME_BUDGET_MS:.1} ms) ===");
                log::info!("Betriebsart      Rendern  Skalieren  PPA-Ausgabe  Arbeit (max)   Budget");
                for (r, &(hi, sm)) in results.iter().zip(&MODES) {
                    let work = r.ms(r.work_us);
                    log::info!(
                        "{}  {:5.1} ms  {:5.1} ms   {:5.1} ms    {:5.1} ms ({:4.1})  {}",
                        mode_name(hi, sm),
                        r.ms(r.render_us),
                        r.ms(r.scale_us),
                        r.ms(r.output_us),
                        work,
                        r.max_work_us as f32 / 1000.0,
                        if work <= FRAME_BUDGET_MS { "OK" } else { "ZU LANG" }
                    );
                }
                log::info!("(Rendern hier = Testbild; im Spiel kommt die 3D-Szene dazu.)");
                (info.hires, info.smooth) = (true, false);
                None
            };
        }

        // --- Touch --------------------------------------------------------------
        let raw = touch.as_ref().map(|t| t.take()).unwrap_or([None; MAX_TOUCH]);
        for (i, (&r, prev)) in raw.iter().zip(prev_touch.iter_mut()).enumerate() {
            info.touches[i] = r.map(|p| (panel_to_game(p, LCD_W as u16, LCD_H as u16, info.flip), p));
            if let (Some(p), None) = (r, *prev) {
                let g = panel_to_game(p, LCD_W as u16, LCD_H as u16, info.flip);
                log::info!("Touch {}: Panel ({}, {}) → Spiel ({}, {})", i + 1, p.0, p.1, g.0, g.1);
                // Tippfelder (nur der erste Finger, nicht während der Messung).
                if i == 0 && bench.is_none() {
                    match testbild::zone_at(g.0, g.1) {
                        Some(Zone::Resolution) => info.hires = !info.hires,
                        Some(Zone::Scaling) => info.smooth = !info.smooth,
                        Some(Zone::Rotation) => info.flip = !info.flip,
                        None => {}
                    }
                }
            }
            if r.is_none() && prev.is_some() {
                log::info!("Touch {}: losgelassen", i + 1);
            }
            *prev = r;
        }

        // --- Knöpfe und GPIO 35 -------------------------------------------------
        let mut buttons = pudding_fw::read_buttons();
        let boot = hw::is_low(BOOT_CANDIDATE);
        if boot {
            buttons |= btn::MENU;
        }
        if buttons != prev_buttons || boot != prev_boot {
            log::info!(
                "Knöpfe: {:06b} (V H L R S B = Bit 0..5), GPIO35 {}",
                buttons,
                if boot { "LOW" } else { "HIGH" }
            );
        }
        (prev_buttons, prev_boot) = (buttons, boot);
        info.buttons = buttons;

        // --- Messwerte (einmal pro Sekunde) ----------------------------------------
        sec_frames += 1;
        if start - sec_start >= 1_000_000 {
            let secs = (start - sec_start) as f32 * 1e-6;
            hz = refreshes().wrapping_sub(sec_refresh) as f32 / secs;
            fps = sec_frames as f32 / secs;
            (sec_start, sec_frames, sec_refresh) = (start, 0, refreshes());
            if let Some(t) = &touch {
                let (reads, changes) = t.counters();
                touch_rate = (reads.wrapping_sub(touch_counters.0) as f32 / secs) as u32;
                int_changes = (changes.wrapping_sub(touch_counters.1) as f32 / secs) as u32;
                touch_counters = (reads, changes);
            }
            if let Some(b) = battery.as_mut() {
                battery_text = match b.read_mv() {
                    Some(mv) => match pudding_core::battery::percent(mv) {
                        Some(p) => line(format_args!("AKKU {:.2} V {p}%", mv as f32 / 1000.0)),
                        None => line(format_args!("AKKU {:.2} V ?", mv as f32 / 1000.0)),
                    },
                    None => line(format_args!("AKKU FEHLER")),
                };
            }
            let [c0, c1] = stats.cores;
            log::info!(
                "{:.1} FPS, Arbeit {:.1} ms (Budget {:.1}), Rendern {:.1}/{:.1}, Skalieren {:.1}/{:.1}, \
                 PPA {:.1}/{:.1} ms, Display {:.2} Hz, {}, Touch {}/s, GPIO2-Wechsel {}/s {}",
                fps,
                work_us as f32 / 1000.0,
                FRAME_BUDGET_MS,
                c0.render_us as f32 / 1000.0,
                c1.render_us as f32 / 1000.0,
                c0.scale_us as f32 / 1000.0,
                c1.scale_us as f32 / 1000.0,
                c0.output_us as f32 / 1000.0,
                c1.output_us as f32 / 1000.0,
                hz,
                mode_name(info.hires, info.smooth),
                touch_rate,
                int_changes,
                battery_text.as_str()
            );
        }

        // --- Bildschirmtext ------------------------------------------------------
        let ms = |us: u32| us as f32 / 1000.0;
        let [c0, c1] = stats.cores;
        let (int_kb, _psram_kb) = hw::free_kb();
        let [p0, p1] = pipeline.placement;
        let work = ms(work_us);
        info.lines = [
            line(format_args!(
                "{fps:.0} FPS  ARBEIT {work:.1} MS  BUDGET {FRAME_BUDGET_MS:.1} {}",
                if work <= FRAME_BUDGET_MS { "OK" } else { "ZU LANG" }
            )),
            line(format_args!(
                "RENDER {:.1}/{:.1} SKALIEREN {:.1}/{:.1} PPA {:.1}/{:.1}",
                ms(c0.render_us),
                ms(c1.render_us),
                ms(c0.scale_us),
                ms(c1.scale_us),
                ms(c0.output_us),
                ms(c1.output_us)
            )),
            match bench {
                Some((i, _)) => line(format_args!("MESSUNG {}/4 LÄUFT: {}", i + 1, mode_name(MODES[i].0, MODES[i].1))),
                None => line(format_args!(
                    "640X360: SCHARF {:.1} MS  WEICH {:.1} MS",
                    results[0].ms(results[0].work_us),
                    results[1].ms(results[1].work_us)
                )),
            },
            line(format_args!("DISPLAY {hz:.1} HZ  CHIP V{}.{} {mhz} MHZ  PSRAM {psram_mb} MB", rev / 100, rev % 100)),
            line(format_args!(
                "FREI {int_kb} KB  PUFFER {}/{} KB  {}",
                p0.internal_kb + p1.internal_kb,
                p0.psram_kb + p1.psram_kb,
                if battery.is_some() { battery_text.as_str() } else { "AKKU AUS" }
            )),
            line(format_args!(
                "TOUCH {} {touch_rate}/S  GPIO2 {int_changes}/S  GPIO35 {}",
                touch.as_ref().map(|t| if t.addr == 0x5D { "0X5D" } else { "0X14" }).unwrap_or("FEHLT"),
                if boot { "LOW" } else { "HIGH" },
            )),
        ];
        info.frame = info.frame.wrapping_add(1);

        // --- Bild -----------------------------------------------------------------
        let (w, h) = if info.hires { (MAX_W, MAX_H) } else { (W, H) };
        let (smooth, flip) = (info.smooth, info.flip);
        let t0 = hw::now_us();
        let fb = disp.acquire();
        let wait = hw::now_us() - t0;
        stats = pipeline.render(&pic, Frame { w, h, smooth, flip, fb, zeilen: None });
        disp.present();
        // Arbeitszeit ohne Warten auf den freien Bildspeicher (VSync).
        work_us = (hw::now_us() - start - wait) as u32;
    }
}
