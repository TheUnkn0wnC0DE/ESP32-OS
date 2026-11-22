//! Mikro-Benchmark der Render-Stufen (kopflos). Nimmt jeweils das beste von
//! mehreren Wiederholungen, damit Störungen durch andere Programme wenig
//! ausmachen. Aufruf: `cargo run --release --example raster_bench`

use spatz::prelude::*;
use spatz::render::raster::{ScreenVertex, Solid, fill_triangle};
use std::hint::black_box;
use std::time::Instant;

fn best_of<F: FnMut()>(reps: usize, inner: usize, mut f: F) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..reps {
        let t = Instant::now();
        for _ in 0..inner {
            f();
        }
        best = best.min(t.elapsed().as_secs_f64() / inner as f64);
    }
    best
}

fn cubes(n: usize, dist: f32, spread: f32) -> (MeshStore, DrawList) {
    let mut store = MeshStore::new();
    let cube = store.add(spatz::render::shapes::box_centered(Vec3::ONE, Color::hex(0xFFCC33)));
    let mut list = DrawList::new();
    for i in 0..n {
        let a = i as f32 * 0.37;
        let p = Vec3::new(a.cos() * spread, a.sin() * spread * 0.5, -dist - (i % 50) as f32 * 0.5);
        list.add(cube, Mat4::from_trs(p, Quat::from_rotation_y(a), Vec3::ONE));
    }
    (store, list)
}

fn main() {
    let v = |x: f32, y: f32, z: f32| ScreenVertex { x, y, z };
    for &(w, h) in &[(480usize, 270usize), (1920, 1080)] {
        println!("--- {w}x{h} ---");
        let px = (w * h) as f64;
        let mut fb = Framebuffer::new(w, h);
        let mut r = Renderer::new();
        let cam = Camera::default();
        let env = Environment::default();

        let t = best_of(15, 20, || {
            fb.clear_depth();
            black_box(&mut fb.depth);
        });
        println!("Tiefenpuffer löschen        : {:7.3} ms  ({:.3} ns/Pixel)", t * 1e3, t * 1e9 / px);

        let (wf, hf) = (w as f32, h as f32);
        let clear = t;
        let t = best_of(15, 10, || {
            fb.clear_depth();
            fill_triangle(&mut fb, v(0.0, 0.0, 0.5), v(wf, 0.0, 0.5), v(0.0, hf, 0.5), &Solid(0xFF0000));
            fill_triangle(&mut fb, v(wf, 0.0, 0.5), v(wf, hf, 0.5), v(0.0, hf, 0.5), &Solid(0xFF0000));
            fill_triangle(&mut fb, v(0.0, 0.0, 0.2), v(wf, 0.0, 0.3), v(0.0, hf, 0.2), &Solid(0x00FF00));
            fill_triangle(&mut fb, v(wf, 0.0, 0.3), v(wf, hf, 0.3), v(0.0, hf, 0.2), &Solid(0x00FF00));
            black_box(&mut fb.color);
        });
        println!("Füllen pro Pixel und Lage   : {:7.3} ns", (t - clear) * 1e9 / (2.0 * px));

        r.begin(&mut fb, &cam, &env);
        for (i, d) in fb.depth.iter_mut().enumerate() {
            *d = if (i / w) < h / 3 { 0 } else { ((i % 4000) as i32) * 4000 };
        }
        let t = best_of(15, 10, || {
            r.finish(&mut fb);
            black_box(&mut fb.color);
        });
        println!("Himmel+Nebel-Durchlauf      : {:7.3} ms  ({:.3} ns/Pixel)", t * 1e3, t * 1e9 / px);

        let mut tris = Vec::new();
        let mut rng = Rng::new(1);
        for _ in 0..20_000 {
            let x = rng.range(0.0, wf - 8.0);
            let y = rng.range(0.0, hf - 8.0);
            tris.push([v(x, y, 0.5), v(x + 7.0, y + 1.0, 0.5), v(x + 2.0, y + 7.0, 0.5)]);
        }
        let t = best_of(10, 3, || {
            for t in &tris {
                fill_triangle(&mut fb, t[0], t[1], t[2], &Solid(0x0000FF));
            }
        });
        println!("Kleine Dreiecke (~25 px)    : {:7.1} ns/Dreieck", t * 1e9 / tris.len() as f64);
    }

    println!("--- Pipeline 480x270, 2000 Würfel ---");
    let mut fb = Framebuffer::new(480, 270);
    let mut r = Renderer::new();
    let cam = Camera::default();
    let env = Environment::default();
    for (name, dist, spread) in
        [("nah (groß)", 6.0f32, 6.0f32), ("fern (klein)", 40.0, 30.0), ("hinter Kamera", -60.0, 6.0)]
    {
        let (store, list) = cubes(2000, dist, spread);
        let t = best_of(10, 5, || {
            r.begin(&mut fb, &cam, &env);
            r.draw_list(&mut fb, &store, &list);
            r.finish(&mut fb);
        });
        let s = r.stats;
        println!(
            "{name:14}: {:6.3} ms | Objekte {}/{} | Dreiecke rein {} gezeichnet {} | {:.1} ns/Dreieck rein",
            t * 1e3,
            s.objects - s.objects_culled,
            s.objects,
            s.tris_in,
            s.tris_drawn,
            t * 1e9 / s.tris_in.max(1) as f64
        );
    }
}
