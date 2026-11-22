//! Minimalbeispiel ohne Fenster: rendert eine kleine Szene und speichert
//! sie als PNG. Aufruf: `cargo run --release --example headless_scene -- out.png`

use spatz::prelude::*;
use spatz::render::shapes;

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "scene.png".into());
    let mut fb = Framebuffer::new(480, 270);
    let mut renderer = Renderer::new();
    let mut store = MeshStore::new();
    let ground = store.add(shapes::cuboid(
        Vec3::new(-8.0, -1.0, -8.0),
        Vec3::new(8.0, 0.0, 8.0),
        [
            Color::hex(0x8a5a3c),
            Color::hex(0x8a5a3c),
            Color::hex(0x6fcf5a),
            Color::hex(0x553322),
            Color::hex(0x8a5a3c),
            Color::hex(0x8a5a3c),
        ],
    ));
    let cube = store.add(shapes::box_centered(Vec3::ONE, Color::hex(0xffcc33)));
    let mut ball = Mesh::new();
    shapes::add_uv_sphere(&mut ball, Vec3::ZERO, 0.8, 10, 7, Color::hex(0xe04060));
    let ball = store.add(ball);
    let shadow = store.add(shapes::disc(12, 0.7, Color::BLACK));

    let mut cam = Camera::default();
    cam.look_at(Vec3::new(5.0, 4.0, 9.0), Vec3::new(0.0, 0.5, 0.0));
    let env = Environment::default();
    let mut list = DrawList::new();
    list.add(ground, Mat4::IDENTITY);
    list.add(cube, Mat4::from_trs(Vec3::new(-1.5, 0.5, 0.0), Quat::from_rotation_y(0.6), Vec3::ONE));
    list.add(ball, Mat4::translation(Vec3::new(1.5, 1.8, 1.0)));
    list.push(shadow, Mat4::translation(Vec3::new(1.5, 0.02, 1.0)), DrawParams::SHADOW);
    for i in 0..40 {
        let a = i as f32 * 0.7;
        list.add(
            cube,
            Mat4::from_trs(Vec3::new(a.cos() * (6.0 + a), 0.5, -a * 3.0), Quat::from_rotation_y(a), Vec3::ONE),
        );
    }

    let t = std::time::Instant::now();
    let n = 500;
    for _ in 0..n {
        renderer.begin(&mut fb, &cam, &env);
        renderer.draw_list(&mut fb, &store, &list);
        renderer.finish(&mut fb);
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
    println!("{} Frames, {:.3} ms/Frame ({:.0} FPS), Stats: {:?}", n, ms, 1000.0 / ms, renderer.stats);
    let (w, h, px) = fb.upscaled(2);
    std::fs::write(&out, spatz::image::encode_png(w, h, &px)).expect("PNG schreiben");
    println!("geschrieben: {out}");
}
