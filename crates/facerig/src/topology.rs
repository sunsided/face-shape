//! Fixed mesh connectivity: a latitude/longitude grid over the head, denser at the face, with
//! cut lines ("slits") for the mouth and both eyes so lips and lids can separate.
//!
//! Every vertex has a canonical *domain* coordinate `(X, Y)`: longitude and latitude in
//! radians, with the face centre at `(0, 0)`, `+X` toward the face's left (`+x`) and `+Y` up.
//! Shape, muscles, weights and landmarks are all defined in this domain, so they stay attached
//! to the same vertices however the identity parameters reshape the head.

use std::f32::consts::{FRAC_PI_2, PI};

/// Mesh resolution.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Detail {
    /// About 14k vertices: smooth enough for close-ups.
    #[default]
    Standard,
    /// About 1.3k vertices, for a faceted low-poly look.
    Low,
}

impl Detail {
    /// Columns around the head (even, so the grid is mirror-symmetric about `X = 0`) and
    /// latitude bands from pole to pole (rows `1..n_lat` hold vertices).
    fn grid(self) -> (usize, usize) {
        match self {
            Detail::Standard => (128, 112),
            Detail::Low => (40, 34),
        }
    }
}
/// Column density at the face relative to the average (lower = denser at the front).
const LON_FRONT_DENSITY: f32 = 0.4;
/// Row density at the equator relative to the average.
const LAT_MID_DENSITY: f32 = 0.6;

// Canonical feature layout (domain radians), snapped to the grid in `Topology::new`.
const EYE_X: f32 = 0.33;
const EYE_Y: f32 = 0.0;
const EYE_HALF_W: f32 = 0.125;
const MOUTH_Y: f32 = -0.56;
const MOUTH_HALF_W: f32 = 0.24;

/// Which side of a slit a vertex sits on.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Side {
    /// Not on a cut line.
    None,
    /// On the cut, owned by the triangles above it (upper lip / upper lid).
    Upper,
    /// The duplicate on the cut, owned by the triangles below it.
    Lower,
}

/// Per-vertex topology data.
#[derive(Copy, Clone, Debug)]
pub struct VertexInfo {
    /// Canonical domain coordinate `(X, Y)`.
    pub dom: [f32; 2],
    pub eye: Side,
    pub mouth: Side,
}

/// Grid-snapped positions of the cut lines.
#[derive(Copy, Clone, Debug)]
pub struct Layout {
    /// `Y` of both eye slits.
    pub eye_y: f32,
    /// `|X|` of the inner and outer eye corners.
    pub eye_inner: f32,
    pub eye_outer: f32,
    /// `Y` of the mouth slit.
    pub mouth_y: f32,
    /// `|X|` of the mouth corners.
    pub mouth_half_w: f32,
}

impl Layout {
    /// `|X|` of the eye centres.
    pub fn eye_x(&self) -> f32 {
        0.5 * (self.eye_inner + self.eye_outer)
    }

    pub fn eye_half_w(&self) -> f32 {
        0.5 * (self.eye_outer - self.eye_inner)
    }
}

/// The face mesh's fixed connectivity.
#[derive(Clone, Debug)]
pub struct Topology {
    pub vertices: Vec<VertexInfo>,
    /// Triangle list, counter-clockwise seen from outside.
    pub indices: Vec<u16>,
    /// `mirror[i]` is the vertex at `(-X, Y)` on the same slit side.
    pub mirror: Vec<u32>,
    /// `twin[i]` is the other copy of a slit vertex (upper <-> lower), or `i` itself.
    pub twin: Vec<u32>,
    pub layout: Layout,
    pub detail: Detail,
}

/// Odd, monotonic density warp on `[-1, 1]`; `a` is the slope at 0.
fn warp(s: f32, a: f32) -> f32 {
    s * (a + (1.0 - a) * s * s)
}

fn lon(n_lon: usize, k: i32) -> f32 {
    PI * warp(k as f32 / (n_lon / 2) as f32, LON_FRONT_DENSITY)
}

fn lat(n_lat: usize, r: usize) -> f32 {
    let s = (2 * r as i32 - n_lat as i32) as f32 / n_lat as f32;
    -FRAC_PI_2 * warp(s, LAT_MID_DENSITY)
}

struct Slit {
    row: usize,
    /// Corner columns (shared); the columns strictly between are cut.
    k0: i32,
    k1: i32,
    eye: bool,
}

impl Topology {
    /// The standard-resolution topology.
    pub fn new() -> Self {
        Self::with_detail(Detail::Standard)
    }

    pub fn with_detail(detail: Detail) -> Self {
        let (n_lon, n_lat) = detail.grid();
        let lon = |k: i32| lon(n_lon, k);
        let lat = |r: usize| lat(n_lat, r);
        let h = (n_lon / 2) as i32;
        let nearest_row = |y: f32| {
            (1..n_lat)
                .min_by(|&a, &b| (lat(a) - y).abs().total_cmp(&(lat(b) - y).abs()))
                .expect("rows")
        };
        let nearest_col = |x: f32| {
            (-h..h)
                .min_by(|&a, &b| (lon(a) - x).abs().total_cmp(&(lon(b) - x).abs()))
                .expect("cols")
        };
        let eye_row = nearest_row(EYE_Y);
        let mouth_row = nearest_row(MOUTH_Y);
        let ki = nearest_col(EYE_X - EYE_HALF_W);
        let ko = nearest_col(EYE_X + EYE_HALF_W);
        let km = nearest_col(MOUTH_HALF_W);
        let slits = [
            Slit {
                row: eye_row,
                k0: -ko,
                k1: -ki,
                eye: true,
            },
            Slit {
                row: eye_row,
                k0: ki,
                k1: ko,
                eye: true,
            },
            Slit {
                row: mouth_row,
                k0: -km,
                k1: km,
                eye: false,
            },
        ];
        let layout = Layout {
            eye_y: lat(eye_row),
            eye_inner: lon(ki),
            eye_outer: lon(ko),
            mouth_y: lat(mouth_row),
            mouth_half_w: lon(km),
        };

        // Grid vertex at row r, column k (k wraps around the back seam).
        let grid =
            |r: usize, k: i32| 1 + (r - 1) * n_lon + (k + h).rem_euclid(n_lon as i32) as usize;
        let slit_of = |r: usize, k: i32| slits.iter().find(|s| s.row == r && s.k0 < k && k < s.k1);

        let mut vertices = Vec::new();
        let mut mirror = Vec::new();
        vertices.push(VertexInfo {
            dom: [0.0, FRAC_PI_2],
            eye: Side::None,
            mouth: Side::None,
        });
        for r in 1..n_lat {
            for j in 0..n_lon {
                let k = j as i32 - h;
                let slit = slit_of(r, k);
                vertices.push(VertexInfo {
                    dom: [lon(k), lat(r)],
                    eye: if slit.is_some_and(|s| s.eye) {
                        Side::Upper
                    } else {
                        Side::None
                    },
                    mouth: if slit.is_some_and(|s| !s.eye) {
                        Side::Upper
                    } else {
                        Side::None
                    },
                });
            }
        }
        let bottom = vertices.len();
        vertices.push(VertexInfo {
            dom: [0.0, -FRAC_PI_2],
            eye: Side::None,
            mouth: Side::None,
        });
        // Lower copies of the cut columns, appended after the grid.
        let mut lower = std::collections::HashMap::new();
        let mut twin: Vec<u32> = (0..vertices.len() as u32).collect();
        for s in &slits {
            for k in s.k0 + 1..s.k1 {
                let up_idx = grid(s.row, k);
                let up = vertices[up_idx];
                lower.insert((s.row, k), vertices.len());
                twin[up_idx] = vertices.len() as u32;
                twin.push(up_idx as u32);
                vertices.push(VertexInfo {
                    eye: if s.eye { Side::Lower } else { Side::None },
                    mouth: if s.eye { Side::None } else { Side::Lower },
                    ..up
                });
            }
        }
        assert!(
            vertices.len() <= u16::MAX as usize,
            "face mesh exceeds u16 indices"
        );

        // Mirror map: (r, k) <-> (r, -k), on the same slit side.
        for (i, v) in vertices.iter().enumerate() {
            let m = if i == 0 || i == bottom {
                i
            } else if i < bottom {
                let r = (i - 1) / n_lon + 1;
                let k = ((i - 1) % n_lon) as i32 - h;
                grid(r, -k)
            } else {
                let (&(r, k), _) = lower.iter().find(|&(_, &idx)| idx == i).expect("copy");
                debug_assert!(v.eye == Side::Lower || v.mouth == Side::Lower);
                lower[&(r, -k)]
            };
            mirror.push(m as u32);
        }

        // Index of (r, k) for a triangle lying below row r (picks the lower copy on a cut).
        let below = |r: usize, k: i32| {
            let k = (k + h).rem_euclid(n_lon as i32) - h;
            match slit_of(r, k) {
                Some(_) => lower[&(r, k)],
                None => grid(r, k),
            }
        };
        let above = grid;

        let mut indices: Vec<u16> = Vec::new();
        let mut tri = |a: usize, b: usize, c: usize| indices.extend([a as u16, b as u16, c as u16]);
        for k in -h..h {
            tri(0, above(1, k), above(1, k + 1));
            tri(below(n_lat - 1, k), bottom, below(n_lat - 1, k + 1));
        }
        for r in 1..n_lat - 1 {
            for k in -h..h {
                let (a, b) = (below(r, k), below(r, k + 1));
                let (c, d) = (above(r + 1, k), above(r + 1, k + 1));
                // Diagonals mirror across X = 0 so the shading is symmetric too.
                if k < 0 {
                    tri(a, c, d);
                    tri(a, d, b);
                } else {
                    tri(a, c, b);
                    tri(b, c, d);
                }
            }
        }

        Self {
            vertices,
            indices,
            mirror,
            twin,
            layout,
            detail,
        }
    }
}

impl Default for Topology {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_symmetric_and_fits_u16() {
        for detail in [Detail::Standard, Detail::Low] {
            check_grid(&Topology::with_detail(detail));
        }
    }

    fn check_grid(t: &Topology) {
        assert!(t.vertices.len() < u16::MAX as usize);
        for (i, &m) in t.mirror.iter().enumerate() {
            let (a, b) = (t.vertices[i], t.vertices[m as usize]);
            assert!(
                (a.dom[0] + b.dom[0]).abs() < 1e-6 || (a.dom[0].abs() - PI).abs() < 1e-5,
                "vertex {i}"
            );
            assert_eq!(a.dom[1], b.dom[1]);
            assert_eq!((a.eye, a.mouth), (b.eye, b.mouth));
            assert_eq!(t.mirror[m as usize] as usize, i);
        }
        assert!(t.indices.iter().all(|&i| (i as usize) < t.vertices.len()));
    }

    #[test]
    fn slits_have_upper_and_lower_copies() {
        for detail in [Detail::Standard, Detail::Low] {
            check_slits(&Topology::with_detail(detail));
        }
    }

    fn check_slits(t: &Topology) {
        let up = t.vertices.iter().filter(|v| v.mouth == Side::Upper).count();
        let low = t.vertices.iter().filter(|v| v.mouth == Side::Lower).count();
        assert!(up >= 4 && up == low, "{:?}: {up} vs {low}", t.detail);
        let up = t.vertices.iter().filter(|v| v.eye == Side::Upper).count();
        let low = t.vertices.iter().filter(|v| v.eye == Side::Lower).count();
        assert!(up >= 4 && up == low, "{:?}: {up} vs {low}", t.detail);
        for (i, &w) in t.twin.iter().enumerate() {
            assert_eq!(t.twin[w as usize] as usize, i);
            assert_eq!(t.vertices[i].dom, t.vertices[w as usize].dom);
        }
        // No triangle mixes an upper and a lower copy.
        for tri in t.indices.chunks(3) {
            let sides: Vec<_> = tri.iter().map(|&i| t.vertices[i as usize].mouth).collect();
            assert!(!(sides.contains(&Side::Upper) && sides.contains(&Side::Lower)));
        }
    }
}
