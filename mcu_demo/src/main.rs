//! Bare-Metal-Beispiel für einen Cortex-M7 (Taschenrechner-Klasse, z. B. die
//! STM32F730-MCU im NumWorks N0110): keine Standardbibliothek, kein
//! Betriebssystem, nur 256 KB RAM. Rendert einen rotierenden, beleuchteten
//! Würfel mit Nebel und Himmel in einen 160×120-Framebuffer.
//!
//! Bauen: `cargo build --release` (in diesem Ordner; Ziel thumbv7em-none-eabihf).
//! Auf echter Hardware würde man `fb.color` nach jedem Frame ans LCD schicken.

#![no_std]
#![no_main]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::panic::PanicInfo;
use spatz::prelude::*;
use spatz::render::shapes;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}

/// Minimaler Bump-Allokator: die Engine alloziert nur beim Aufbau, nicht pro Frame.
struct Bump {
    next: UnsafeCell<usize>,
    heap: UnsafeCell<[u8; HEAP_SIZE]>,
}
// SAFETY: einkernig, keine Interrupts, die allozieren.
unsafe impl Sync for Bump {}

const HEAP_SIZE: usize = 200 * 1024;

unsafe impl GlobalAlloc for Bump {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // SAFETY: exklusiver Zugriff (einkernig, keine Nebenläufigkeit).
        let next = unsafe { &mut *self.next.get() };
        let start = (*next + l.align() - 1) & !(l.align() - 1);
        if start + l.size() > HEAP_SIZE {
            return core::ptr::null_mut();
        }
        *next = start + l.size();
        // SAFETY: `start + size` liegt innerhalb des Heap-Arrays.
        unsafe { (self.heap.get() as *mut u8).add(start) }
    }
    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
}

#[global_allocator]
static ALLOC: Bump = Bump { next: UnsafeCell::new(0), heap: UnsafeCell::new([0; HEAP_SIZE]) };

#[unsafe(link_section = ".vector_table.reset")]
#[unsafe(no_mangle)]
static RESET_VECTOR: unsafe extern "C" fn() -> ! = reset;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn reset() -> ! {
    let mut fb = Framebuffer::new(160, 120);
    let mut renderer = Renderer::new();
    let mut store = MeshStore::new();
    let cube = store.add(shapes::box_centered(Vec3::ONE, Color::hex(0xFFCC33)));
    let ground = store.add(shapes::cuboid(
        Vec3::new(-4.0, -1.5, -4.0),
        Vec3::new(4.0, -0.5, 4.0),
        [Color::hex(0xF78FBF); 6],
    ));
    let mut list = DrawList::new();
    let mut cam = Camera::default();
    cam.look_at(Vec3::new(0.0, 2.0, 5.0), Vec3::ZERO);
    let env = Environment::default();
    let mut t = 0.0f32;
    loop {
        t += 1.0 / 60.0;
        list.clear();
        list.add(ground, Mat4::IDENTITY);
        list.add(cube, Mat4::from_trs(Vec3::ZERO, Quat::from_yaw_pitch_roll(t, t * 0.7, 0.0), Vec3::ONE));
        renderer.begin(&mut fb, &cam, &env);
        renderer.draw_list(&mut fb, &store, &list);
        renderer.finish(&mut fb);
        // Hier ginge das Bild ans Display; volatile, damit nichts wegoptimiert wird.
        // SAFETY: gültiger Zeiger in den Framebuffer.
        unsafe { core::ptr::read_volatile(fb.color.as_ptr()) };
    }
}
