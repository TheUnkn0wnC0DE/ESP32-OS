//! Polygon-Clipping im homogenen Raum (Sutherland–Hodgman).
//!
//! Geclippt wird nur gegen die Near-Ebene und ein großzügiges Guard-Band;
//! alles innerhalb des Guard-Bands schneidet der Rasterizer billig pro Zeile
//! ab. Schnittpunkte werden immer vom inneren Vertex aus berechnet, damit
//! benachbarte Dreiecke identische Kantenpunkte erhalten.

/// Vertex im (vereinfachten) Clip-Raum: `x' = f·x_view`, `y' = -f·y_view`,
/// `w = Tiefe entlang der Blickrichtung`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClipVertex {
    pub x: f32,
    pub y: f32,
    pub w: f32,
}

/// Maximale Vertexzahl nach dem Clippen (3 + 1 pro Ebene, großzügig).
pub const MAX_CLIP_VERTS: usize = 12;

/// Ebene `a·x + b·y + c·w + d >= 0` = innen.
pub type ClipPlane = [f32; 4];

#[inline(always)]
fn dist(v: &ClipVertex, p: &ClipPlane) -> f32 {
    p[0] * v.x + p[1] * v.y + p[2] * v.w + p[3]
}

#[inline(always)]
fn lerp(a: &ClipVertex, b: &ClipVertex, t: f32) -> ClipVertex {
    ClipVertex { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t, w: a.w + (b.w - a.w) * t }
}

/// Clippt `src` gegen `plane` nach `dst`; gibt die neue Länge zurück.
pub fn clip_against(src: &[ClipVertex], dst: &mut [ClipVertex; MAX_CLIP_VERTS], plane: &ClipPlane) -> usize {
    let n = src.len();
    let mut out = 0;
    for i in 0..n {
        let cur = &src[i];
        let nxt = &src[if i + 1 == n { 0 } else { i + 1 }];
        let dc = dist(cur, plane);
        let dn = dist(nxt, plane);
        let cin = dc >= 0.0;
        if cin && out < MAX_CLIP_VERTS {
            dst[out] = *cur;
            out += 1;
        }
        if cin != (dn >= 0.0) && out < MAX_CLIP_VERTS {
            dst[out] = if cin { lerp(cur, nxt, dc / (dc - dn)) } else { lerp(nxt, cur, dn / (dn - dc)) };
            out += 1;
        }
    }
    out
}

/// Clippt ein Polygon nacheinander gegen alle Ebenen. Ergebnis liegt in
/// `a[..n]`.
pub fn clip_polygon(a: &mut [ClipVertex; MAX_CLIP_VERTS], mut n: usize, planes: &[ClipPlane]) -> usize {
    let mut b = [ClipVertex::default(); MAX_CLIP_VERTS];
    for p in planes {
        if n < 3 {
            return 0;
        }
        n = clip_against(&a[..n], &mut b, p);
        a[..n].copy_from_slice(&b[..n]);
    }
    if n < 3 { 0 } else { n }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_clip_cuts_triangle() {
        let mut poly = [ClipVertex::default(); MAX_CLIP_VERTS];
        poly[0] = ClipVertex { x: 0.0, y: 0.0, w: 5.0 };
        poly[1] = ClipVertex { x: 1.0, y: 0.0, w: -5.0 };
        poly[2] = ClipVertex { x: 0.0, y: 1.0, w: 5.0 };
        let near = 0.1;
        let n = clip_polygon(&mut poly, 3, &[[0.0, 0.0, 1.0, -near]]);
        assert_eq!(n, 4);
        assert!(poly[..n].iter().all(|v| v.w >= near - 1e-5));
    }
}
