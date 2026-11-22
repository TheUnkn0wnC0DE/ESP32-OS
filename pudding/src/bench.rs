//! Kopflose Modi: Benchmark (Autopilot spielt das Level, Frametimes werden
//! gemessen) und Screenshots. Öffnen nie ein Fenster.

use crate::game::{Phase, PuddingGame};
use crate::level::LevelDef;
use spatz::app::{Control, Game};
use spatz::input::Input;
use spatz::render::overlay::draw_perf_overlay;
use spatz::render::{Framebuffer, RenderStats};
use spatz::time::FrameStats;
use std::time::Instant;

/// Ergebnis eines Benchmark-Laufs.
#[derive(Clone, Debug)]
pub struct BenchResult {
    pub width: usize,
    pub height: usize,
    pub frames: usize,
    pub avg_ms: f64,
    pub p50_ms: f64,
    pub p99_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub update_ms: f64,
    pub render_ms: f64,
    pub tris_drawn: f64,
    pub finish_time: Option<f32>,
    pub deaths: u32,
    pub cherries: u32,
    pub total_cherries: u32,
}

impl BenchResult {
    pub fn fps(&self) -> f64 {
        1000.0 / self.avg_ms
    }
}

/// Simuliert `frames` Bilder à 1/60 s Spielzeit (2 Physikschritte à 120 Hz)
/// und misst Simulation + Rendering + HUD/Overlay pro Bild.
pub fn run_one(def: &LevelDef, w: usize, h: usize, max_frames: usize, stop_at_finish: bool) -> BenchResult {
    let mut game = PuddingGame::new(def, true);
    let mut fb = Framebuffer::new(w, h);
    let input = Input::new();
    let mut ctl = Control::default();
    let mut stats = FrameStats::new();
    let dt = 1.0 / 120.0;
    let mut frame_ms = Vec::with_capacity(max_frames);
    let (mut upd_sum, mut ren_sum, mut tris_sum) = (0.0f64, 0.0f64, 0.0f64);
    let mut finish_time = None;
    let mut frames = 0;
    for _ in 0..max_frames {
        let t0 = Instant::now();
        game.update(&input, dt, &mut ctl);
        game.update(&input, dt, &mut ctl);
        let t1 = Instant::now();
        let rs: RenderStats = game.render(&mut fb, 1.0);
        draw_perf_overlay(&mut fb, &stats, &rs, &["BENCHMARK"]);
        let t2 = Instant::now();
        let upd = (t1 - t0).as_secs_f64() * 1000.0;
        let ren = (t2 - t1).as_secs_f64() * 1000.0;
        stats.record((upd + ren) as f32, upd as f32, ren as f32, 0.0);
        frame_ms.push(upd + ren);
        upd_sum += upd;
        ren_sum += ren;
        tris_sum += rs.tris_drawn as f64;
        frames += 1;
        if finish_time.is_none() {
            if let (Phase::Finished { .. }, Some(t)) = (game.phase, game.finish_time) {
                finish_time = Some(t);
            }
        }
        if stop_at_finish && matches!(game.phase, Phase::Finished { t } if t > 3.0) {
            break;
        }
    }
    let mut sorted = frame_ms.clone();
    sorted.sort_by(f64::total_cmp);
    let pct = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize];
    let n = frames.max(1) as f64;
    BenchResult {
        width: w,
        height: h,
        frames,
        avg_ms: frame_ms.iter().sum::<f64>() / n,
        p50_ms: pct(0.5),
        p99_ms: pct(0.99),
        min_ms: sorted[0],
        max_ms: *sorted.last().unwrap(),
        update_ms: upd_sum / n,
        render_ms: ren_sum / n,
        tris_drawn: tris_sum / n,
        finish_time,
        deaths: game.player.deaths,
        cherries: game.player.cherries,
        total_cherries: game.total_cherries,
    }
}

/// Misst die Kosten des GDI-Hochskalierens (wie im Fenster) in eine
/// unsichtbare Speicher-DC.
#[cfg(windows)]
pub fn present_cost_ms(src_w: usize, src_h: usize, dst_w: usize, dst_h: usize, n: usize) -> Option<f64> {
    let mut blit = spatz::platform::win32::OffscreenBlit::new(dst_w, dst_h)?;
    let mut fb = Framebuffer::new(src_w, src_h);
    for (i, p) in fb.color.iter_mut().enumerate() {
        *p = (i as u32).wrapping_mul(2_654_435_761) & 0xFFFFFF;
    }
    blit.blit(&fb);
    let t = Instant::now();
    for _ in 0..n {
        blit.blit(&fb);
    }
    Some(t.elapsed().as_secs_f64() * 1000.0 / n as f64)
}

pub fn run(def: &LevelDef, max_frames: usize, resolutions: &[(usize, usize)]) {
    println!("Pudding-Panik Benchmark (headless, 1 Thread, Autopilot spielt das Level)");
    println!("Pro Bild: 2 Physikschritte (120 Hz) + Rendering + HUD + Perf-Overlay\n");
    println!(
        "{:>10} | {:>6} | {:>7} | {:>7} | {:>7} | {:>7} | {:>7} | {:>7} | {:>7} | {:>7}",
        "Auflösung", "Bilder", "Ø ms", "min ms", "p50 ms", "p99 ms", "max ms", "Ø FPS", "Physik", "Render"
    );
    println!("{}", "-".repeat(100));
    let mut first: Option<BenchResult> = None;
    for &(w, h) in resolutions {
        let r = run_one(def, w, h, max_frames, true);
        println!(
            "{:>10} | {:>6} | {:>7.3} | {:>7.3} | {:>7.3} | {:>7.3} | {:>7.3} | {:>7.0} | {:>7.3} | {:>7.3}",
            format!("{}x{}", r.width, r.height),
            r.frames,
            r.avg_ms,
            r.min_ms,
            r.p50_ms,
            r.p99_ms,
            r.max_ms,
            r.fps(),
            r.update_ms,
            r.render_ms
        );
        println!("{:>10}   Ø {:.0} gezeichnete Dreiecke pro Bild", "", r.tris_drawn);
        if first.is_none() {
            first = Some(r);
        }
    }
    if let Some(r) = first {
        match r.finish_time {
            Some(t) => println!(
                "\nAutopilot: Level geschafft in {:.1} s Spielzeit, {} Platscher, {}/{} Kirschen.",
                t, r.deaths, r.cherries, r.total_cherries
            ),
            None => println!("\nAutopilot: Ziel NICHT erreicht ({} Platscher).", r.deaths),
        }
    }
    #[cfg(windows)]
    for &(sw, sh, dw, dh) in &[(480, 270, 1920, 1080), (320, 240, 960, 720)] {
        if let Some(ms) = present_cost_ms(sw, sh, dw, dh, 300) {
            println!("GDI-Present {sw}x{sh} -> {dw}x{dh} (unsichtbare Speicher-DC): {ms:.3} ms/Bild");
        }
    }
}

/// Zeitprofil eines Frames, aufgeteilt in Phasen (Mittel über den Lauf).
pub fn profile(def: &LevelDef, w: usize, h: usize) {
    let mut game = PuddingGame::new(def, true);
    let mut fb = Framebuffer::new(w, h);
    let input = Input::new();
    let mut ctl = Control::default();
    let mut stats = FrameStats::new();
    let dt = 1.0 / 120.0;
    let names = [
        "Simulation",
        "Szene vorbereiten",
        "begin (Tiefe löschen)",
        "Geometrie+Raster",
        "Himmel+Nebel",
        "HUD",
        "Perf-Overlay",
    ];
    let mut acc = [0.0f64; 7];
    let (mut frames, mut tris_in, mut tris_drawn) = (0usize, 0u64, 0u64);
    while frames < 9000 {
        let mut t = [Instant::now(); 8];
        game.update(&input, dt, &mut ctl);
        game.update(&input, dt, &mut ctl);
        t[1] = Instant::now();
        let cam = game.prepare(1.0);
        t[2] = Instant::now();
        {
            let (r, store, list, env) = game.parts();
            r.begin(&mut fb, &cam, env);
            t[3] = Instant::now();
            r.draw_list(&mut fb, store, list);
            t[4] = Instant::now();
            r.finish(&mut fb);
            t[5] = Instant::now();
        }
        game.draw_hud(&mut fb);
        t[6] = Instant::now();
        let rs = game.renderer.stats;
        draw_perf_overlay(&mut fb, &stats, &rs, &[]);
        t[7] = Instant::now();
        let total = (t[7] - t[0]).as_secs_f32() * 1000.0;
        stats.record(total, 0.0, total, 0.0);
        for i in 0..7 {
            acc[i] += (t[i + 1] - t[i]).as_secs_f64() * 1000.0;
        }
        tris_in += rs.tris_in as u64;
        tris_drawn += rs.tris_drawn as u64;
        frames += 1;
        if matches!(game.phase, Phase::Finished { t } if t > 3.0) {
            break;
        }
    }
    let n = frames as f64;
    let sum: f64 = acc.iter().sum();
    println!("Profil {w}x{h}, {frames} Bilder, Ø {:.3} ms/Bild", sum / n);
    for (name, a) in names.iter().zip(acc) {
        println!("  {:<24} {:7.4} ms  ({:4.1} %)", name, a / n, a / sum * 100.0);
    }
    println!("  Dreiecke pro Bild: {:.0} rein, {:.0} gezeichnet", tris_in as f64 / n, tris_drawn as f64 / n);
}

/// Spielt das Level mit dem Autopiloten ohne Rendering durch und
/// protokolliert Routenschritte, Tode und das Ergebnis (für Level-Tuning).
/// Gibt die Zielzeit zurück, falls erreicht.
pub fn trace(def: &LevelDef, verbose: bool) -> Option<f32> {
    let mut game = PuddingGame::new(def, true);
    let input = Input::new();
    let mut ctl = Control::default();
    let dt = 1.0 / 120.0;
    let mut last_step = usize::MAX;
    let mut last_deaths = 0;
    for i in 0..(240 * 120) {
        game.update(&input, dt, &mut ctl);
        let step = game.autopilot.as_ref().map(|a| a.step).unwrap_or(0);
        let p = &game.player.body;
        if verbose && step != last_step {
            let s = game.world.route.get(step).map(|s| format!("{s:?}")).unwrap_or_else(|| "ENDE".into());
            println!(
                "t={:6.2}s  Schritt {:2} {:<44} pos=({:6.2},{:5.2},{:7.2}) boden={}",
                i as f32 * dt,
                step,
                s,
                p.pos.x,
                p.pos.y,
                p.pos.z,
                p.on_ground
            );
            last_step = step;
        }
        if game.player.deaths != last_deaths {
            last_deaths = game.player.deaths;
            println!(
                "t={:6.2}s  !!! PLATSCHER #{} bei Schritt {} pos=({:.2},{:.2},{:.2}) hitze={:.2}",
                i as f32 * dt,
                last_deaths,
                step,
                p.pos.x,
                p.pos.y,
                p.pos.z,
                game.player.melt
            );
        }
        if let Some(t) = game.finish_time {
            println!(
                "ZIEL nach {:.1} s, {} Platscher, {}/{} Kirschen",
                t, game.player.deaths, game.player.cherries, game.total_cherries
            );
            return Some(t);
        }
    }
    println!("Ziel nicht erreicht (Schritt {last_step}).");
    None
}

/// Rendert Screenshots an markanten Stellen des Autopilot-Laufs.
pub fn screenshots(
    def: &LevelDef,
    dir: &std::path::Path,
    w: usize,
    h: usize,
) -> std::io::Result<Vec<std::path::PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let mut game = PuddingGame::new(def, true);
    let mut fb = Framebuffer::new(w, h);
    let input = Input::new();
    let mut ctl = Control::default();
    let mut stats = FrameStats::new();
    let dt = 1.0 / 120.0;
    // (Spielzeit in s, Name)
    let shots = [
        (1.0, "01_intro"),
        (4.0, "02_kekse"),
        (7.5, "03_donut"),
        (11.0, "04_waffel"),
        (14.2, "05_trampolin"),
        (20.0, "06_bruecke"),
        (23.0, "07_karussell"),
        (31.0, "08_aufzug"),
    ];
    let mut out = Vec::new();
    let mut next = 0;
    let mut finish_shot = false;
    let mut t = 0.0f32;
    for _ in 0..(200 * 60) {
        let t0 = Instant::now();
        game.update(&input, dt, &mut ctl);
        game.update(&input, dt, &mut ctl);
        let t1 = Instant::now();
        let rs = game.render(&mut fb, 1.0);
        let t2 = Instant::now();
        let ms = |a: Instant, b: Instant| (b - a).as_secs_f32() * 1000.0;
        stats.record(ms(t0, t2), ms(t0, t1), ms(t1, t2), 0.0);
        t += 2.0 * dt;
        let take = if next < shots.len() && t >= shots[next].0 {
            next += 1;
            Some(shots[next - 1].1.to_string())
        } else if !finish_shot && matches!(game.phase, Phase::Finished { t } if t > 3.0) {
            finish_shot = true;
            Some("09_ziel".to_string())
        } else {
            None
        };
        if let Some(name) = take {
            let label = format!("{}X{} HEADLESS", fb.width(), fb.height());
            draw_perf_overlay(&mut fb, &stats, &rs, &[label.as_str()]);
            let (uw, uh, px) = fb.upscaled(2);
            let path = dir.join(format!("{name}.png"));
            std::fs::write(&path, spatz::image::encode_png(uw, uh, &px))?;
            out.push(path);
        }
        if finish_shot {
            break;
        }
    }
    Ok(out)
}

/// Bildrate des GIF-Clips (Verzögerung 4/100 s pro Bild).
const CLIP_FPS: f32 = 25.0;
/// So lange steht der Pudding am Anfang still (Kicheranfall ab 2,2 s).
const CLIP_IDLE: f32 = 3.4;

/// Spielt den Clip ab und ruft `each` pro Bild auf: erst steht der Pudding
/// still (Kamera nah von vorn, er kichert), dann übernimmt der Autopilot und
/// die Kamera schwenkt schräg hinter ihn. Deterministisch.
fn clip_frames(def: &LevelDef, w: usize, h: usize, frames: usize, mut each: impl FnMut(usize, &Framebuffer)) {
    use spatz::math::{PI, Vec3, angle_diff, cos, sin};
    let mut game = PuddingGame::new(def, false);
    game.rig.intro = 0.0;
    game.title_t = 0.0;
    // Gesicht zur Sonne drehen, damit es im Bild schön hell ist.
    game.player.yaw = 2.2;
    game.player.prev_yaw = 2.2;
    let mut fb = Framebuffer::new(w, h);
    let input = Input::new();
    let mut ctl = Control::default();
    let dt = 1.0 / 120.0;
    let (mut acc, mut t) = (0.0f32, 0.0f32);
    let mut target = game.player.render_pos(1.0) + Vec3::new(0.0, 0.5, 0.0);
    // Kamerawinkel um den Pudding (wie `CameraRig::yaw`: 0 = dahinter).
    let mut cam_yaw = game.player.yaw + PI - 0.35;
    let mut dist = 2.7f32;
    for i in 0..frames {
        acc += 1.0 / CLIP_FPS;
        while acc >= dt {
            game.update(&input, dt, &mut ctl);
            acc -= dt;
            t += dt;
            if game.autopilot.is_none() && t >= CLIP_IDLE {
                let mut ap = crate::autopilot::Autopilot::new();
                ap.resync(&game.world, game.player.body.pos);
                game.autopilot = Some(ap);
            }
        }
        let alpha = acc / dt;
        let want = game.player.render_pos(alpha) + Vec3::new(0.0, 0.5, 0.0);
        target.x += (want.x - target.x) * 0.35;
        target.z += (want.z - target.z) * 0.35;
        target.y += (want.y - target.y) * 0.18;
        // Erst schräg von vorn aufs Gesicht, dann schräg hinter die Laufrichtung.
        let (want_yaw, want_dist) =
            if t < CLIP_IDLE { (game.player.yaw + PI - 0.35, 2.7) } else { (game.rig.yaw + 0.85, 3.8) };
        cam_yaw += angle_diff(cam_yaw, want_yaw) * 0.05;
        dist += (want_dist - dist) * 0.05;
        let y = cam_yaw;
        let eye = target + Vec3::new(sin(y) * dist * 0.94, dist * 0.36, cos(y) * dist * 0.94);
        let mut cam = game.rig.camera(alpha);
        cam.look_at(eye, target);
        game.prepare(alpha);
        game.relight_player(alpha, eye);
        let (r, store, list, env) = game.parts();
        r.begin(&mut fb, &cam, env);
        r.draw_list(&mut fb, store, list);
        r.finish(&mut fb);
        game.draw_hud(&mut fb);
        each(i, &fb);
    }
}

/// Nimmt einen Clip als animiertes GIF auf (kopflos, ohne Fenster). Zwei
/// identische Durchläufe: der erste sammelt die Farben für die Palette, der
/// zweite kodiert. Liefert (Bilder, Bytes).
pub fn record_gif(
    def: &LevelDef,
    path: &std::path::Path,
    w: usize,
    h: usize,
    seconds: f32,
) -> std::io::Result<(usize, usize)> {
    use spatz::image::{GifEncoder, GifPalette, RGB555_BINS, rgb555};
    let frames = (seconds * CLIP_FPS) as usize;
    let mut hist = vec![0u32; RGB555_BINS];
    clip_frames(def, w, h, frames, |i, fb| {
        if i % 2 == 0 {
            for &p in &fb.color {
                hist[rgb555(p)] += 1;
            }
        }
    });
    let pal = GifPalette::from_histogram(&hist);
    let mut enc = GifEncoder::new(w, h, &pal);
    let delay = (100.0 / CLIP_FPS) as u16;
    clip_frames(def, w, h, frames, |_, fb| enc.frame(&fb.color, delay));
    let bytes = enc.finish();
    std::fs::write(path, &bytes)?;
    Ok((frames, bytes.len()))
}

/// Einzelbilder des Clips als PNG (jedes `every`-te Bild), z. B. zum Prüfen.
pub fn clip_pngs(
    def: &LevelDef,
    dir: &std::path::Path,
    w: usize,
    h: usize,
    seconds: f32,
    every: usize,
) -> std::io::Result<usize> {
    std::fs::create_dir_all(dir)?;
    let mut res = Ok(0);
    clip_frames(def, w, h, (seconds * CLIP_FPS) as usize, |i, fb| {
        if i % every.max(1) == 0 && res.is_ok() {
            let p = dir.join(format!("clip_{i:04}.png"));
            res = std::fs::write(p, spatz::image::encode_png(w, h, &fb.color)).map(|_| i + 1);
        }
    });
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level;

    #[test]
    fn autopilot_beats_the_level_without_dying() {
        let def = level::parse(level::BUILTIN).unwrap();
        let t = trace(&def, false).expect("Autopilot muss das Ziel erreichen");
        assert!(t < 60.0, "zu langsam: {t} s");
    }

    #[test]
    fn renders_at_odd_resolutions_without_panicking() {
        let def = level::parse(level::BUILTIN).unwrap();
        for (w, h) in [(17usize, 16usize), (64, 36), (333, 187), (480, 270)] {
            let mut game = PuddingGame::new(&def, true);
            let mut fb = Framebuffer::new(w, h);
            let input = Input::new();
            let mut ctl = Control::default();
            for _ in 0..90 {
                game.update(&input, 1.0 / 120.0, &mut ctl);
                game.render(&mut fb, 0.5);
            }
            let first = fb.color[0];
            assert!(fb.color.iter().any(|&c| c != first), "{w}x{h}: Bild ist einfarbig");
        }
    }

    #[test]
    fn restart_resets_the_run() {
        let def = level::parse(level::BUILTIN).unwrap();
        let mut game = PuddingGame::new(&def, true);
        let input = Input::new();
        let mut ctl = Control::default();
        for _ in 0..1200 {
            game.update(&input, 1.0 / 120.0, &mut ctl);
        }
        assert!(game.player.cherries > 0);
        game.restart();
        assert_eq!(game.player.cherries, 0);
        assert_eq!(game.time, 0.0);
        assert!(game.world.items.iter().all(|i| !i.taken));
    }
}
