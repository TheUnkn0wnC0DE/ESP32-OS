//! TEMPORÄR (Optik-Analyse): Bilder des echten Renderers wie auf dem Board
//! (640×360 RGB565, 8-Zeilen-Streifen) als PNG, dazu ×2 bilinear vergrößert
//! (PPA „weich“) – einmal über das ganze Bild, einmal je 8-Zeilen-Block mit
//! Randwiederholung (so sieht der PPA jeden Streifen einzeln).
//!
//! Aufruf (im Ordner esp32):
//!   set BILDER_DIR=...; cargo test --profile sim -q -p pudding-core --test bilder -- --ignored --nocapture

use pudding_core::app::{App, MAX_MESH_VERTICES};
use pudding_core::campaign::Difficulty;
use pudding_core::gfx::to_rgb888;
use pudding_core::input::{Input, btn};
use pudding_core::render::{Scratch, Target};
use std::path::{Path, PathBuf};

const DT: f32 = 1.0 / 55.0;

fn out_dir() -> PathBuf {
    let d = std::env::var("BILDER_DIR").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir().join("zz_bilder"));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Wie die Firmware: begin_frame, dann 8-Zeilen-Streifen.
fn frame(app: &mut App) -> (Vec<u16>, usize, usize) {
    let (w, h) = app.render_size();
    app.begin_frame(w, h);
    let (mut color, mut depth) = (vec![0u16; w * h], vec![0u16; w * h]);
    let mut s = Scratch::with_capacity(MAX_MESH_VERTICES);
    let rows = (640 * 8 / w).max(1);
    let mut rest = Target::new(&mut color, &mut depth, w, h);
    for y in (rows..h).step_by(rows).chain([h]) {
        let (mut band, tail) = rest.split(y);
        app.render_band(&mut s, &mut band);
        rest = tail;
    }
    // Aufwand des Bildes (Vergleich vorher/nachher): aufbereitete Dreiecke,
    // gezeichnete Dreiecke, Zeilenstücke, gefüllte Pixel (inkl. verdeckter).
    let st = s.stats;
    println!("  Aufwand: prims {} tris {} zeilen {} pixel {}", app.debug_prims(), st.tris_drawn, st.rows, st.px);
    (color, w, h)
}

fn rgb(p: u16) -> [f32; 3] {
    let v = to_rgb888(p);
    [((v >> 16) & 255) as f32, ((v >> 8) & 255) as f32, (v & 255) as f32]
}

fn pack(c: [f32; 3]) -> u32 {
    let q = |x: f32| (x.round().clamp(0.0, 255.0)) as u32;
    (q(c[0]) << 16) | (q(c[1]) << 8) | q(c[2])
}

/// ×2 bilinear, Pixelmitte auf Pixelmitte; `block` = Zeilen je PPA-Auftrag
/// (Randwiederholung an den Blockgrenzen), 0 = ganzes Bild.
fn bilinear2(src: &[u16], w: usize, h: usize, block: usize) -> Vec<u32> {
    let (ow, oh) = (w * 2, h * 2);
    let mut out = vec![0u32; ow * oh];
    for oy in 0..oh {
        let fy = (oy as f32 + 0.5) / 2.0 - 0.5;
        let sy = oy / 2;
        let (b0, b1) = if block == 0 { (0, h - 1) } else { (sy / block * block, (sy / block * block + block - 1).min(h - 1)) };
        let fy = fy.clamp(b0 as f32, b1 as f32);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(b1);
        let ty = fy - y0 as f32;
        for ox in 0..ow {
            let fx = ((ox as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (w - 1) as f32);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(w - 1);
            let tx = fx - x0 as f32;
            let (a, b, c, d) = (rgb(src[y0 * w + x0]), rgb(src[y0 * w + x1]), rgb(src[y1 * w + x0]), rgb(src[y1 * w + x1]));
            let mut r = [0.0; 3];
            for k in 0..3 {
                let top = a[k] + (b[k] - a[k]) * tx;
                let bot = c[k] + (d[k] - c[k]) * tx;
                r[k] = top + (bot - top) * ty;
            }
            out[oy * ow + ox] = pack(r);
        }
    }
    out
}

fn save(dir: &Path, name: &str, img: &(Vec<u16>, usize, usize)) {
    let (c, w, h) = img;
    let rgb: Vec<u32> = c.iter().map(|&p| to_rgb888(p)).collect();
    std::fs::write(dir.join(format!("{name}.png")), spatz::image::encode_png(*w, *h, &rgb)).unwrap();
    // Rohdaten (RGB565, little endian) für die Auswertung in Python.
    let raw: Vec<u8> = c.iter().flat_map(|p| p.to_le_bytes()).collect();
    std::fs::write(dir.join(format!("{name}.565")), raw).unwrap();
    let b = bilinear2(c, *w, *h, 8);
    std::fs::write(dir.join(format!("{name}_ppa_weich.png")), spatz::image::encode_png(w * 2, h * 2, &b)).unwrap();
    println!("{}", dir.join(name).display());
}

fn board_app() -> App {
    let mut app = App::new(None);
    app.konsole = true;
    app.touch_always = true;
    app
}

fn wait(app: &mut App, n: usize, input: Input) {
    for _ in 0..n {
        app.update(input, DT);
    }
}

#[test]
#[ignore]
fn bilder_pudding() {
    let dir = out_dir();
    let only: Option<Vec<usize>> =
        std::env::var("BILDER_KAPITEL").ok().map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect());
    // Titel und Hauptmenü.
    if only.is_none() {
        let mut app = board_app();
        wait(&mut app, 60, Input::default());
        save(&dir, "00_titel", &frame(&mut app));
        wait(&mut app, 1, Input::buttons(btn::JUMP));
        wait(&mut app, 40, Input::default());
        save(&dir, "01_hauptmenue", &frame(&mut app));
    }
    let steps: Vec<usize> =
        std::env::var("BILDER_SCHRITTE").ok().map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or(vec![400, 900]);
    for k in 0..8 {
        if let Some(o) = &only
            && !o.contains(&k)
        {
            continue;
        }
        let mut app = board_app();
        app.start_chapter(k, Difficulty::Easy);
        app.set_autopilot(true);
        let mut done = 0;
        for &n in &steps {
            wait(&mut app, n - done, Input::default());
            done = n;
            save(&dir, &format!("k{}_s{:04}", k + 1, n), &frame(&mut app));
        }
        if only.is_none() && k == 0 {
            // Pause-Menü über der Szene.
            wait(&mut app, 1, Input::buttons(btn::MENU));
            wait(&mut app, 20, Input::default());
            save(&dir, "k1_pause", &frame(&mut app));
        }
    }
}
