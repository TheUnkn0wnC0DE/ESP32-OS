//! Bitgenaue Regressionsprüfung („Golden Master“) und Performance-Messung.
//!
//! * `--golden-save DIR`: spielt das Level mit dem Autopiloten bei mehreren
//!   Auflösungen durch, rendert JEDEN Frame (Szene + HUD + Perf-Overlay mit
//!   deterministischen Zahlen) und speichert pro Frame einen Hash des
//!   Farbpuffers sowie einen Hash des Spielzustands. Dazu Referenz-PNGs.
//! * `--golden-check DIR`: wiederholt das und vergleicht bitgenau.
//! * `--perf N`: N komplette Läufe mit Zeitmessung pro Phase, Ausgabe Median.

use crate::game::{Phase, PuddingGame};
use crate::level::LevelDef;
use spatz::app::{Control, Game};
use spatz::input::Input;
use spatz::render::Framebuffer;
use spatz::render::overlay::PerfOverlay;
use spatz::time::FrameStats;
use std::fmt::Write as _;
use std::time::Instant;

/// Auflösungen der Referenz: Standard, Taschenrechner-Format, „krumme“
/// Größe (testet SIMD-Reste) und groß (lange Spans).
pub const GOLDEN_RES: [(usize, usize); 4] = [(480, 270), (320, 240), (333, 187), (1280, 720)];
/// Frames, von denen zusätzlich PNGs gespeichert werden (bei 480x270).
const PNG_FRAMES: [usize; 6] = [30, 400, 800, 1200, 1700, 2150];
const MAX_FRAMES: usize = 2400;

/// Schneller 64-Bit-Hash über den Farbpuffer (2 Pixel pro Schritt).
pub fn frame_hash(px: &[u32]) -> u64 {
    let mut h: u64 = 0x9E37_79B9_7F4A_7C15 ^ px.len() as u64;
    let mut chunks = px.chunks_exact(2);
    for c in &mut chunks {
        let v = (c[0] as u64) | ((c[1] as u64) << 32);
        h = (h ^ v).wrapping_mul(0x0000_0100_0000_01B3).rotate_left(29);
    }
    for &p in chunks.remainder() {
        h = (h ^ p as u64).wrapping_mul(0x0000_0100_0000_01B3).rotate_left(29);
    }
    h ^ (h >> 31)
}

/// Hash des Spielzustands (Positionen/Geschwindigkeiten bitgenau).
fn state_hash(g: &PuddingGame) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    let mut put = |v: u32| h = (h ^ v as u64).wrapping_mul(0x0000_0100_0000_01B3);
    let b = &g.player.body;
    for f in [b.pos.x, b.pos.y, b.pos.z, b.vel.x, b.vel.y, b.vel.z, g.player.melt, g.player.yaw, g.time, g.world.time] {
        put(f.to_bits());
    }
    put(g.player.cherries);
    put(g.player.deaths);
    put(b.on_ground as u32);
    for d in &g.world.donuts {
        put(d.pos.x.to_bits());
        put(d.pos.y.to_bits());
        put(d.pos.z.to_bits());
        put(d.alive as u32);
    }
    for f in [g.rig.pos.x, g.rig.pos.y, g.rig.pos.z, g.rig.yaw] {
        put(f.to_bits());
    }
    put(match g.phase {
        Phase::Playing => 0,
        Phase::Dead { .. } => 1,
        Phase::Finished { .. } => 2,
    });
    h
}

/// Deterministische „Messwerte“, damit das Perf-Overlay reproduzierbar ist.
fn fake_record(stats: &mut FrameStats, i: usize) {
    let f = 1.5 + (i % 13) as f32 * 0.37;
    stats.record(f, 0.04 + (i % 3) as f32 * 0.01, f * 0.7, 0.2);
}

/// Ein kompletter Referenzlauf; ruft `each` pro Frame mit (Frame, Puffer,
/// Zustandshash) auf.
fn golden_run(def: &LevelDef, w: usize, h: usize, mut each: impl FnMut(usize, &Framebuffer, u64)) {
    let mut game = PuddingGame::new(def, true);
    let mut fb = Framebuffer::new(w, h);
    let input = Input::new();
    let mut ctl = Control::default();
    let mut stats = FrameStats::new();
    let mut overlay = PerfOverlay::new();
    let label = format!("{w}X{h} VSYNC AN (F2)");
    for i in 0..MAX_FRAMES {
        game.update(&input, 1.0 / 120.0, &mut ctl);
        game.update(&input, 1.0 / 120.0, &mut ctl);
        let rs = game.render(&mut fb, 0.5);
        fake_record(&mut stats, i);
        overlay.draw(&mut fb, &stats, &rs, &[label.as_str()]);
        each(i, &fb, state_hash(&game));
        if matches!(game.phase, Phase::Finished { t } if t > 4.0) {
            break;
        }
    }
}

fn reference_lines(def: &LevelDef, png_dir: Option<&std::path::Path>) -> std::io::Result<Vec<String>> {
    let mut lines = Vec::new();
    for (w, h) in GOLDEN_RES {
        let mut io_err = None;
        golden_run(def, w, h, |i, fb, sh| {
            lines.push(format!("{w}x{h} {i:5} {:016x} {sh:016x}", frame_hash(&fb.color)));
            if let (Some(dir), true) = (png_dir, (w, h) == GOLDEN_RES[0] && PNG_FRAMES.contains(&i)) {
                let p = dir.join(format!("ref_{w}x{h}_{i:05}.png"));
                if let Err(e) = std::fs::write(p, spatz::image::encode_png(w, h, &fb.color)) {
                    io_err = Some(e);
                }
            }
        });
        if let Some(e) = io_err {
            return Err(e);
        }
    }
    Ok(lines)
}

pub fn golden_save(def: &LevelDef, dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let lines = reference_lines(def, Some(dir))?;
    std::fs::write(dir.join("reference.txt"), lines.join("\n") + "\n")?;
    println!("Referenz gespeichert: {} Frames in {}", lines.len(), dir.display());
    Ok(())
}

/// Vergleicht bitgenau. Gibt `true` zurück, wenn alles identisch ist.
pub fn golden_check(def: &LevelDef, dir: &std::path::Path) -> std::io::Result<bool> {
    let want = std::fs::read_to_string(dir.join("reference.txt"))?;
    let want: Vec<&str> = want.lines().collect();
    let got = reference_lines(def, None)?;
    let mut bad = 0;
    let mut first = String::new();
    for (i, g) in got.iter().enumerate() {
        if want.get(i) != Some(&g.as_str()) {
            if bad == 0 {
                let _ = write!(
                    first,
                    "erste Abweichung: erwartet '{}' bekommen '{}'",
                    want.get(i).unwrap_or(&"<fehlt>"),
                    g
                );
            }
            bad += 1;
        }
    }
    if got.len() != want.len() {
        bad += 1;
        let _ = write!(first, " (Anzahl Frames {} statt {})", got.len(), want.len());
    }
    if bad == 0 {
        println!("GOLDEN OK: {} Frames bitgenau identisch (Bild + Spielzustand)", got.len());
        Ok(true)
    } else {
        println!("GOLDEN FEHLER: {bad} Abweichungen; {first}");
        Ok(false)
    }
}

pub const PHASE_NAMES: [&str; 7] =
    ["Simulation", "Szene vorbereiten", "begin", "Geometrie+Raster", "Himmel+Nebel", "HUD", "Perf-Overlay"];

/// Zeiten eines Laufs: pro Frame und Phase in ms.
pub struct RunTimes {
    pub frames: Vec<[f32; 7]>,
    /// Heap-Allokationen nach der Aufwärmphase (Ziel: 0).
    pub steady_allocs: usize,
}

/// Frames, nach denen alle Puffer ihre Endgröße erreicht haben.
const WARMUP_FRAMES: usize = 120;

impl RunTimes {
    /// Mittelwert pro Phase über alle Frames.
    fn means(&self) -> [f64; 7] {
        let mut m = [0.0f64; 7];
        for f in &self.frames {
            for k in 0..7 {
                m[k] += f[k] as f64;
            }
        }
        m.map(|v| v / self.frames.len().max(1) as f64)
    }
}

/// Ein kompletter Lauf mit Zeitmessung pro Phase.
pub fn perf_run(def: &LevelDef, w: usize, h: usize) -> RunTimes {
    let mut game = PuddingGame::new(def, true);
    let mut fb = Framebuffer::new(w, h);
    let input = Input::new();
    let mut ctl = Control::default();
    let mut stats = FrameStats::new();
    let mut overlay = PerfOverlay::new();
    let mut out = Vec::with_capacity(MAX_FRAMES);
    let mut allocs_at_warmup = 0;
    for i in 0..MAX_FRAMES {
        if i == WARMUP_FRAMES {
            allocs_at_warmup = crate::alloc_count::thread_allocs();
        }
        let t0 = Instant::now();
        game.update(&input, 1.0 / 120.0, &mut ctl);
        game.update(&input, 1.0 / 120.0, &mut ctl);
        let t1 = Instant::now();
        let cam = game.prepare(0.5);
        let t2 = Instant::now();
        let (t3, t4, t5);
        {
            let (r, store, list, env) = game.parts();
            r.begin(&mut fb, &cam, env);
            t3 = Instant::now();
            r.draw_list(&mut fb, store, list);
            t4 = Instant::now();
            r.finish(&mut fb);
            t5 = Instant::now();
        }
        game.draw_hud(&mut fb);
        let t6 = Instant::now();
        fake_record(&mut stats, i);
        let rs = game.renderer.stats;
        overlay.draw(&mut fb, &stats, &rs, &["480X270 VSYNC AN (F2)"]);
        let t7 = Instant::now();
        let ts = [t0, t1, t2, t3, t4, t5, t6, t7];
        let mut f = [0.0f32; 7];
        for k in 0..7 {
            f[k] = (ts[k + 1] - ts[k]).as_secs_f32() * 1000.0;
        }
        out.push(f);
        if matches!(game.phase, Phase::Finished { t } if t > 4.0) {
            break;
        }
    }
    // `out` hat seine Kapazität von Anfang an → zählt nicht mit.
    let steady_allocs = crate::alloc_count::thread_allocs() - allocs_at_warmup;
    RunTimes { frames: out, steady_allocs }
}

static STEADY_ALLOCS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn last_steady_allocs() -> usize {
    STEADY_ALLOCS.load(std::sync::atomic::Ordering::Relaxed)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) * 0.5
    }
}

/// `runs` Läufe pro Auflösung. Ausgabe:
/// * Median der Lauf-Mittelwerte (Wanduhr, inkl. Störungen durch andere Programme)
/// * „bereinigt“: pro Frame das Minimum über alle Läufe, dann gemittelt –
///   Störungen verlängern Frames nur, daher ist das sehr stabil (für A/B-Tests).
pub fn perf(def: &LevelDef, runs: usize, resolutions: &[(usize, usize)]) {
    for &(w, h) in resolutions {
        let results: Vec<RunTimes> = (0..runs).map(|_| perf_run(def, w, h)).collect();
        STEADY_ALLOCS
            .store(results.iter().map(|r| r.steady_allocs).max().unwrap_or(0), std::sync::atomic::Ordering::Relaxed);
        let totals: Vec<f64> = results.iter().map(|r| r.means().iter().sum()).collect();
        let med_total = median(totals.clone());
        let n = results[0].frames.len();
        let mut clean = [0.0f64; 7];
        let mut clean_total = 0.0f64;
        for i in 0..n {
            let mut best_total = f32::MAX;
            for k in 0..7 {
                clean[k] += results.iter().map(|r| r.frames[i][k]).fold(f32::MAX, f32::min) as f64;
            }
            for r in &results {
                best_total = best_total.min(r.frames[i].iter().sum());
            }
            clean_total += best_total as f64;
        }
        let clean = clean.map(|c| c / n as f64);
        let clean_total = clean_total / n as f64;
        println!(
            "PERF {w}x{h}: Median {:.4} ms/Frame ({:.0} FPS) | bereinigt {:.4} ms ({:.0} FPS) | {} Frames, {} Läufe: {}",
            med_total,
            1000.0 / med_total,
            clean_total,
            1000.0 / clean_total,
            n,
            runs,
            totals.iter().map(|t| format!("{t:.4}")).collect::<Vec<_>>().join(" ")
        );
        println!("CLEAN {w}x{h}: {clean_total:.5}");
        for (k, name) in PHASE_NAMES.iter().enumerate() {
            let m = median(results.iter().map(|r| r.means()[k]).collect());
            println!(
                "   {:<20} Median {:8.4} ms | bereinigt {:8.4} ms ({:4.1} %)",
                name,
                m,
                clean[k],
                clean[k] / clean_total * 100.0
            );
        }
    }
    println!(
        "Heap: Spitze {:.0} KB (inkl. Messdaten des Benchmarks), Allokationen pro Frame nach dem Aufwärmen: {}",
        crate::alloc_count::peak_bytes() as f64 / 1024.0,
        last_steady_allocs()
    );
    #[cfg(windows)]
    if let Some((peak, private)) = spatz::platform::win32::process_memory() {
        println!(
            "Speicher: Spitze Arbeitsspeicher {:.1} MB, privat {:.1} MB",
            peak as f64 / 1048576.0,
            private as f64 / 1048576.0
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level;

    #[test]
    fn no_heap_allocations_per_frame_after_warmup() {
        let def = level::parse(level::BUILTIN).unwrap();
        let r = perf_run(&def, 320, 240);
        assert!(r.frames.len() > 1000);
        assert_eq!(r.steady_allocs, 0, "Heap-Allokationen im laufenden Spiel");
    }

    #[test]
    fn frame_hash_detects_single_pixel_change() {
        let mut a = vec![0x123456u32; 333];
        let h0 = frame_hash(&a);
        a[200] ^= 1;
        assert_ne!(h0, frame_hash(&a));
    }
}
