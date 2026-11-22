//! Scanline-Rasterizer für Dreiecke mit Ganzzahl-Tiefenpuffer.
//!
//! * Pixelzentren liegen bei `.5`; Top-Left-Füllregel → keine Lücken und
//!   keine doppelt gezeichneten Pixel an gemeinsamen Kanten.
//! * Kanten werden pro Zeile direkt (nicht inkrementell) aus dem oberen
//!   Endpunkt ausgewertet, dadurch sind gemeinsame Kanten bitgenau gleich.
//! * Tiefe = `near/w` (linear im Bildraum) als Fixpunkt; pro Pixel nur eine
//!   Ganzzahl-Addition und ein Vergleich. Die Span-Schleifen sind so
//!   formuliert, dass LLVM sie vektorisieren kann.

use super::framebuffer::{DEPTH_SCALE, Framebuffer};
use crate::color::{avg_packed, darken_half};

/// Projizierter Vertex: Pixelkoordinaten + `z = near / w` in `(0, 1]`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScreenVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Füllart für Spans. `prepare` wird einmal pro Dreieck mit dem Tiefen-
/// zuwachs pro Pixel aufgerufen (dort entstehen z. B. SIMD-Konstanten), der
/// zurückgegebene [`SpanWriter`] dann für jede Zeile.
pub trait SpanFill {
    type Writer: SpanWriter;
    fn prepare(&self, dz: i32) -> Self::Writer;
}

/// Schreibt eine horizontale Pixelspanne; `z` = Tiefe des ersten Pixels.
pub trait SpanWriter {
    fn fill(&self, color: &mut [u32], depth: &mut [i32], z: i32);
}

/// Opake Farbe mit Tiefentest und Tiefenschreiben.
pub struct Solid(pub u32);

/// Vorbereiteter Writer für [`Solid`].
pub struct SolidSpan {
    c: u32,
    dz: i32,
    #[cfg(target_arch = "x86_64")]
    simd: SolidSse2,
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy)]
struct SolidSse2 {
    cv: core::arch::x86_64::__m128i,
    /// (0, dz, 2dz, 3dz)
    lanes: core::arch::x86_64::__m128i,
    /// 4·dz in jeder Spur
    step: core::arch::x86_64::__m128i,
}

impl SpanFill for Solid {
    type Writer = SolidSpan;
    #[inline(always)]
    fn prepare(&self, dz: i32) -> SolidSpan {
        #[cfg(target_arch = "x86_64")]
        let simd = {
            use core::arch::x86_64::*;
            // SAFETY: SSE2 ist Teil der x86_64-Basis-ISA (reine Registeroperationen).
            unsafe {
                SolidSse2 {
                    cv: _mm_set1_epi32(self.0 as i32),
                    lanes: _mm_setr_epi32(0, dz, dz.wrapping_mul(2), dz.wrapping_mul(3)),
                    step: _mm_set1_epi32(dz.wrapping_mul(4)),
                }
            }
        };
        SolidSpan {
            c: self.0,
            dz,
            #[cfg(target_arch = "x86_64")]
            simd,
        }
    }
}

impl SpanWriter for SolidSpan {
    #[inline(always)]
    fn fill(&self, color: &mut [u32], depth: &mut [i32], z: i32) {
        let n = color.len().min(depth.len());
        #[cfg(target_arch = "x86_64")]
        let (start, z) = fill_solid_sse2(&mut color[..n], &mut depth[..n], z, self.dz, &self.simd);
        #[cfg(not(target_arch = "x86_64"))]
        let start = 0;
        let mut z = z;
        let (c, dz) = (self.c, self.dz);
        for (cp, zp) in color[start..n].iter_mut().zip(depth[start..n].iter_mut()) {
            let old = *zp;
            let pass = z > old;
            *zp = if pass { z } else { old };
            *cp = if pass { c } else { *cp };
            z = z.wrapping_add(dz);
        }
    }
}

/// SSE2-Pfad (auf x86_64 immer verfügbar): 4 Pixel pro Schritt. Blöcke, in
/// denen kein Pixel den Tiefentest besteht, schreiben gar nicht.
/// Gibt die Anzahl erledigter Pixel und das `z` für den Rest zurück.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn fill_solid_sse2(color: &mut [u32], depth: &mut [i32], z: i32, dz: i32, k: &SolidSse2) -> (usize, i32) {
    use core::arch::x86_64::*;
    let n = color.len().min(depth.len());
    if n < 4 {
        return (0, z);
    }
    let blocks = n / 4;
    // SAFETY: SSE2 ist Teil der x86_64-Basis-ISA. Alle Zugriffe liegen in
    // `[0, blocks*4) ⊆ [0, n)` beider Slices; Laden/Speichern unausgerichtet.
    unsafe {
        let mut zv = _mm_add_epi32(_mm_set1_epi32(z), k.lanes);
        let dp = depth.as_mut_ptr() as *mut __m128i;
        let cp = color.as_mut_ptr() as *mut __m128i;
        let block = |dpb: *mut __m128i, cpb: *mut __m128i, zv: __m128i| {
            let old = _mm_loadu_si128(dpb);
            let mask = _mm_cmpgt_epi32(zv, old);
            _mm_storeu_si128(dpb, _mm_or_si128(_mm_and_si128(mask, zv), _mm_andnot_si128(mask, old)));
            let oc = _mm_loadu_si128(cpb);
            _mm_storeu_si128(cpb, _mm_or_si128(_mm_and_si128(mask, k.cv), _mm_andnot_si128(mask, oc)));
        };
        for b in 0..blocks {
            block(dp.add(b), cp.add(b), zv);
            zv = _mm_add_epi32(zv, k.step);
        }
        if n % 4 != 0 {
            // Rest als überlappender letzter Block: Schon geschriebene Pixel
            // bestehen den strikten Test `z > alt` mit ihrem eigenen z nicht
            // erneut → exakt dasselbe Ergebnis wie eine Restschleife, aber ohne
            // schwer vorhersagbare Sprünge.
            let o = n - 4;
            let zl = _mm_add_epi32(_mm_set1_epi32(z.wrapping_add(dz.wrapping_mul(o as i32))), k.lanes);
            block(depth.as_mut_ptr().add(o) as *mut __m128i, color.as_mut_ptr().add(o) as *mut __m128i, zl);
        }
    }
    (n, z.wrapping_add(dz.wrapping_mul(n as i32)))
}

/// Abdunkeln (Blob-Schatten): Tiefentest, aber kein Tiefenschreiben.
pub struct Shadow;

/// Vorbereiteter Writer für [`Shadow`].
pub struct ShadowSpan(i32);

impl SpanFill for Shadow {
    type Writer = ShadowSpan;
    #[inline(always)]
    fn prepare(&self, dz: i32) -> ShadowSpan {
        ShadowSpan(dz)
    }
}

impl SpanWriter for ShadowSpan {
    #[inline(always)]
    fn fill(&self, color: &mut [u32], depth: &mut [i32], mut z: i32) {
        for (cp, zp) in color.iter_mut().zip(depth.iter()) {
            let pass = z > *zp;
            *cp = if pass { darken_half(*cp) } else { *cp };
            z = z.wrapping_add(self.0);
        }
    }
}

/// 50 % durchscheinend: Tiefentest, kein Tiefenschreiben.
pub struct Blend50(pub u32);

/// Vorbereiteter Writer für [`Blend50`].
pub struct Blend50Span(u32, i32);

impl SpanFill for Blend50 {
    type Writer = Blend50Span;
    #[inline(always)]
    fn prepare(&self, dz: i32) -> Blend50Span {
        Blend50Span(self.0, dz)
    }
}

impl SpanWriter for Blend50Span {
    #[inline(always)]
    fn fill(&self, color: &mut [u32], depth: &mut [i32], mut z: i32) {
        for (cp, zp) in color.iter_mut().zip(depth.iter()) {
            let pass = z > *zp;
            *cp = if pass { avg_packed(*cp, self.0) } else { *cp };
            z = z.wrapping_add(self.1);
        }
    }
}

/// Schnelles `ceil` für Bildschirmkoordinaten (vermeidet libm-Aufrufe auf
/// CPUs ohne SSE4.1). Gültig für |x| < 2^31.
#[inline(always)]
pub fn ceil_i32(x: f32) -> i32 {
    let i = x as i32;
    if (i as f32) < x { i + 1 } else { i }
}

/// Rastert ein Dreieck (beliebige Orientierung) mit dem Span-Füller `s`.
/// Gibt die Anzahl bearbeiteter Zeilen zurück (für Statistik).
/// Auf x86_64 `inline(always)`, damit die Funktion in die mehrfach
/// kompilierten Render-Pfade (Basis/AVX2) eingebettet wird.
#[cfg_attr(target_arch = "x86_64", inline(always))]
pub fn fill_triangle<S: SpanFill>(
    fb: &mut Framebuffer,
    v0: ScreenVertex,
    v1: ScreenVertex,
    v2: ScreenVertex,
    s: &S,
) -> u32 {
    fb.mark_depth_dirty();
    // Pixelzentren auf ganze Zahlen legen.
    let sh = |v: ScreenVertex| ScreenVertex { x: v.x - 0.5, y: v.y - 0.5, z: v.z };
    let (v0, v1, v2) = (sh(v0), sh(v1), sh(v2));
    let (a, b, c) = if v0.y.is_nan() || v1.y.is_nan() || v2.y.is_nan() {
        // Entartete Eingabe: klassisches (stabiles) Vertauschen.
        let (mut a, mut b, mut c) = (v0, v1, v2);
        if a.y > b.y {
            core::mem::swap(&mut a, &mut b);
        }
        if b.y > c.y {
            core::mem::swap(&mut b, &mut c);
        }
        if a.y > b.y {
            core::mem::swap(&mut a, &mut b);
        }
        (a, b, c)
    } else {
        // Sprungfreie, stabile Sortierung nach y über Ränge (gleiche
        // Reihenfolge wie das Vertauschen, auch bei Gleichstand).
        let (ya, yb, yc) = (v0.y, v1.y, v2.y);
        let ra = (yb < ya) as usize + (yc < ya) as usize;
        let rb = (ya <= yb) as usize + (yc < yb) as usize;
        let rc = (ya <= yc) as usize + (yb <= yc) as usize;
        let mut o = [ScreenVertex::default(); 4];
        o[ra] = v0;
        o[rb] = v1;
        o[rc] = v2;
        (o[0], o[1], o[2])
    };

    let total_h = c.y - a.y;
    if total_h.is_nan() || total_h <= 0.0 {
        return 0;
    }
    let w = fb.width() as i32;
    let h = fb.height() as i32;
    let y_start = ceil_i32(a.y).max(0);
    let y_end = ceil_i32(c.y).min(h);
    if y_start >= y_end {
        return 0;
    }
    // Trifft das Dreieck in X überhaupt ein Pixelzentrum im Bild? (Spart bei
    // winzigen/fernen Dreiecken das komplette Setup inkl. Divisionen.)
    let x_lo = ceil_i32(a.x.min(b.x).min(c.x)).max(0);
    let x_hi = ceil_i32(a.x.max(b.x).max(c.x)).min(w);
    if x_lo >= x_hi {
        return 0;
    }
    let y_mid = ceil_i32(b.y).clamp(y_start, y_end);

    // Tiefe als Ebene z(x, y) = a.z + dzdx·(x - a.x) + dzdy·(y - a.y).
    let (e1x, e1y) = (b.x - a.x, b.y - a.y);
    let (e2x, e2y) = (c.x - a.x, c.y - a.y);
    let area = e1x * e2y - e1y * e2x;
    if crate::math::abs(area) < 1e-6 {
        return 0;
    }
    let inv_area = 1.0 / area;
    let dz1 = (b.z - a.z) * DEPTH_SCALE;
    let dz2 = (c.z - a.z) * DEPTH_SCALE;
    let dzdx = (dz1 * e2y - dz2 * e1y) * inv_area;
    let dzdy = (dz2 * e1x - dz1 * e2x) * inv_area;
    // Tiefe am Span-Anfang wird pro Zeile relativ zu Vertex `a` in f32
    // ausgewertet (kein f64 → schnell auch auf FPUs mit nur einfacher
    // Genauigkeit, z. B. Cortex-M4F/M7); pro Pixel dann reine Ganzzahl-Addition.
    let za = a.z * DEPTH_SCALE;
    let dz_step = if dzdx >= 0.0 { (dzdx + 0.5) as i32 } else { (dzdx - 0.5) as i32 };

    // Kanten als 16.16-Fixpunkt-DDA, jeweils ab dem oberen Endpunkt der Kante
    // ausgewertet → gemeinsame Kanten benachbarter Dreiecke sind bitgleich.
    const FX: f32 = 65536.0;
    let fx = |v: f32| (v * FX) as i32;
    let slope = |dx: f32, dy: f32| if dy > 0.0 { (dx / dy).clamp(-30000.0, 30000.0) } else { 0.0 };
    let s_ac = slope(e2x, total_h);
    // Liegt der mittlere Vertex links der langen Kante?
    let mid_left = b.x < a.x + s_ac * e1y;

    // Puffergrößen einmal prüfen, danach Zeilenzugriff ohne Bounds-Checks.
    let width = w as usize;
    if fb.color.len() < width * h as usize || fb.depth.len() < width * h as usize {
        return 0;
    }
    let writer = s.prepare(dz_step);
    let cbase = fb.color.as_mut_ptr();
    let zbase = fb.depth.as_mut_ptr();
    let mut x_long = fx(a.x + s_ac * (y_start as f32 - a.y));
    let d_long = fx(s_ac);
    let row = |y: i32, x_long: i32, x_short: i32| {
        let (xl, xr) = if mid_left { (x_short, x_long) } else { (x_long, x_short) };
        let x0 = ((xl as i64 + 0xFFFF) >> 16).max(0) as i32;
        let x1 = ((xr as i64 + 0xFFFF) >> 16).min(w as i64) as i32;
        if x0 < x1 {
            let z0 = (za + dzdx * (x0 as f32 - a.x) + dzdy * (y as f32 - a.y)) as i32;
            let (x0, x1) = (x0 as usize, x1 as usize);
            let o = y as usize * width + x0;
            // SAFETY: 0 ≤ y < h und 0 ≤ x0 < x1 ≤ w, Pufferlänge ≥ w·h (oben
            // geprüft) → der Bereich liegt in Zeile y; sonst lebt keine Referenz.
            let (crow, zrow) = unsafe {
                (
                    core::slice::from_raw_parts_mut(cbase.add(o), x1 - x0),
                    core::slice::from_raw_parts_mut(zbase.add(o), x1 - x0),
                )
            };
            writer.fill(crow, zrow, z0);
        }
    };
    // Obere Hälfte: kurze Kante a→b (Steigung nur berechnen, wenn gebraucht).
    if y_mid > y_start {
        let s_ab = slope(e1x, e1y);
        let mut x_short = fx(a.x + s_ab * (y_start as f32 - a.y));
        let d_short = fx(s_ab);
        for y in y_start..y_mid {
            row(y, x_long, x_short);
            x_long = x_long.wrapping_add(d_long);
            x_short = x_short.wrapping_add(d_short);
        }
    }
    // Untere Hälfte: kurze Kante b→c (ab ihrem oberen Endpunkt b).
    if y_end > y_mid {
        let s_bc = slope(c.x - b.x, c.y - b.y);
        let mut x_short = fx(b.x + s_bc * (y_mid as f32 - b.y));
        let d_short = fx(s_bc);
        for y in y_mid..y_end {
            row(y, x_long, x_short);
            x_long = x_long.wrapping_add(d_long);
            x_short = x_short.wrapping_add(d_short);
        }
    }
    (y_end - y_start) as u32
}

/// Rechteck mit konstanter Tiefe (Partikel/Sprites), Tiefentest + -schreiben.
pub fn fill_rect_depth(fb: &mut Framebuffer, x0: i32, y0: i32, x1: i32, y1: i32, z: i32, c: u32) {
    fb.mark_depth_dirty();
    let x0 = x0.max(0) as usize;
    let y0 = y0.max(0) as usize;
    let x1 = (x1.max(0) as usize).min(fb.width());
    let y1 = (y1.max(0) as usize).min(fb.height());
    if x0 >= x1 {
        return;
    }
    let writer = Solid(c).prepare(0);
    for y in y0..y1 {
        let (crow, zrow) = fb.rows_mut(y);
        writer.fill(&mut crow[x0..x1], &mut zrow[x0..x1], z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32) -> ScreenVertex {
        ScreenVertex { x, y, z: 0.5 }
    }

    #[test]
    fn shared_edges_cover_every_pixel_exactly_once() {
        // Ein Quadrat aus zwei Dreiecken + ein Fächer mit ungeraden Kanten:
        // jeder Pixel darf genau einmal getroffen werden.
        struct Count;
        struct CountW;
        impl SpanFill for Count {
            type Writer = CountW;
            fn prepare(&self, _dz: i32) -> CountW {
                CountW
            }
        }
        impl SpanWriter for CountW {
            fn fill(&self, color: &mut [u32], _d: &mut [i32], _z: i32) {
                for c in color {
                    *c += 1;
                }
            }
        }
        let mut fb = Framebuffer::new(64, 64);
        let center = v(31.3, 30.7);
        let n = 13;
        let pts: std::vec::Vec<ScreenVertex> = (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * core::f32::consts::TAU;
                v(31.3 + 25.0 * a.cos(), 30.7 + 25.0 * a.sin())
            })
            .collect();
        for i in 0..n {
            fill_triangle(&mut fb, center, pts[i], pts[(i + 1) % n], &Count);
        }
        assert!(fb.color.iter().all(|&c| c <= 1), "Pixel doppelt gezeichnet");
        // Innenbereich muss lückenlos sein.
        for y in 20..40 {
            for x in 20..40 {
                assert_eq!(fb.pixel(x, y), 1, "Lücke bei {x},{y}");
            }
        }
    }

    #[test]
    fn depth_test_keeps_nearest() {
        let mut fb = Framebuffer::new(16, 16);
        let near = |x, y| ScreenVertex { x, y, z: 0.9 };
        let far = |x, y| ScreenVertex { x, y, z: 0.1 };
        fill_triangle(&mut fb, near(0.0, 0.0), near(16.0, 0.0), near(0.0, 16.0), &Solid(1));
        fill_triangle(&mut fb, far(0.0, 0.0), far(16.0, 0.0), far(0.0, 16.0), &Solid(2));
        assert_eq!(fb.pixel(2, 2), 1);
        fill_triangle(&mut fb, far(0.0, 0.0), far(16.0, 16.0), far(16.0, 0.0), &Solid(3));
        assert_eq!(fb.pixel(14, 2), 3);
    }
}
