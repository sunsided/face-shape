//! Colored skin regions (brows, lips) as decals: small meshes laid out in the canonical domain
//! and bound to the skin by barycentric coordinates. Their outlines stay smooth whatever the
//! skin's triangle grid, and they follow every identity, expression and pose change.

use crate::landmarks::BROW;
use crate::math::{Vec3, add, normalize, scale, smoothstep};
use crate::topology::{Layout, Topology};

/// A colored part of the skin.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Region {
    Brows,
    Lips,
}

/// Height of the upper and lower lip's outer (vermilion) border above and below the mouth
/// line at domain `x`; zero at and beyond the corners. The outer-lip landmarks sit on it.
pub(crate) fn lip_outline(l: &Layout, x: f32) -> (f32, f32) {
    let s = (1.0 - (x / l.mouth_half_w).powi(2)).max(0.0).sqrt();
    // Cupid's bow: peaks either side of a dip at the centre.
    let upper = s * (0.06 + 0.012 * (-((x.abs() - 0.06) / 0.04).powi(2)).exp())
        - 0.012 * (-(x / 0.03).powi(2)).exp() * s;
    (upper.max(0.0), 0.08 * s)
}

/// Brow centre line `Y` at `|X|` = `ax`, through the brow landmarks.
fn brow_center(ax: f32) -> f32 {
    BROW.windows(2)
        .find(|w| ax <= w[0].0 && ax >= w[1].0)
        .map_or_else(
            || if ax > BROW[0].0 { BROW[0].1 } else { BROW[4].1 },
            |w| {
                let t = (ax - w[1].0) / (w[0].0 - w[1].0);
                w[1].1 + t * (w[0].1 - w[1].1)
            },
        )
}

/// A grid of `cols × rows` domain points, `at(u, v)` with `u, v ∈ [0, 1]`, as quads.
fn strip(
    cols: usize,
    rows: usize,
    at: impl Fn(f32, f32) -> [f32; 2],
    pts: &mut Vec<[f32; 2]>,
    tris: &mut Vec<[u16; 3]>,
    flip: bool,
) {
    let base = pts.len();
    for c in 0..=cols {
        for r in 0..=rows {
            pts.push(at(c as f32 / cols as f32, r as f32 / rows as f32));
        }
    }
    let id = |c: usize, r: usize| (base + c * (rows + 1) + r) as u16;
    for c in 0..cols {
        for r in 0..rows {
            let (a, b, d, e) = (id(c, r), id(c + 1, r), id(c, r + 1), id(c + 1, r + 1));
            if flip {
                tris.extend([[a, d, b], [b, d, e]]);
            } else {
                tris.extend([[a, b, d], [b, e, d]]);
            }
        }
    }
}

/// Domain layout of `region`: points and triangles (counter-clockwise seen from outside).
fn layout(topo: &Topology, region: Region) -> (Vec<[f32; 2]>, Vec<[u16; 3]>) {
    let l = topo.layout;
    let (mut pts, mut tris) = (Vec::new(), Vec::new());
    match region {
        Region::Brows => {
            for side in [-1.0f32, 1.0] {
                // u runs inner (0) to outer (1) end; v bottom (0) to top (1).
                let at = |u: f32, v: f32| {
                    let ax = 0.12 + 0.4 * u;
                    let taper = (0.55 + 0.45 * smoothstep(0.0, 0.2, u))
                        * (1.0 - 0.85 * smoothstep(0.6, 1.0, u));
                    let half = 0.03 * taper;
                    [side * ax, brow_center(ax) + half * (2.0 * v - 1.0)]
                };
                // Mirrored strips need the opposite winding.
                strip(28, 3, at, &mut pts, &mut tris, side < 0.0);
            }
        }
        Region::Lips => {
            let (my, mw) = (l.mouth_y, l.mouth_half_w);
            // Inset from the slit so each lip binds to its own side of the cut.
            let eps = 0.002;
            for upper in [true, false] {
                let at = |u: f32, v: f32| {
                    let x = mw * (2.0 * u - 1.0);
                    let (hu, hl) = lip_outline(&l, x);
                    let y = if upper {
                        my + eps + (hu - eps).max(0.0) * v
                    } else {
                        my - eps - (hl - eps).max(0.0) * v
                    };
                    [x, y]
                };
                strip(40, 4, at, &mut pts, &mut tris, !upper);
            }
        }
    }
    (pts, tris)
}

/// A decal point: a skin triangle and barycentric weights.
#[derive(Copy, Clone, Debug)]
struct Bind {
    tri: [usize; 3],
    w: [f32; 3],
}

/// A region as a decal mesh bound to the skin.
#[derive(Clone, Debug)]
pub struct Decal {
    binds: Vec<Bind>,
    /// Triangle list into the decal's own vertices.
    pub indices: Vec<u16>,
}

fn barycentric(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> Option<[f32; 3]> {
    let det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if det.abs() < 1e-12 {
        return None;
    }
    let w0 = ((b[1] - c[1]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[1] - c[1])) / det;
    let w1 = ((c[1] - a[1]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[1] - c[1])) / det;
    let w = [w0, w1, 1.0 - w0 - w1];
    w.iter().all(|&x| x >= -1e-5).then_some(w)
}

impl Decal {
    /// Lay out `region` and bind each of its points to the skin triangle containing it in the
    /// domain.
    pub fn new(topo: &Topology, region: Region) -> Self {
        let (pts, tris) = layout(topo, region);
        let skin = topo.indices.as_chunks::<3>().0;
        let dom = |i: u16| topo.vertices[usize::from(i)].dom;
        let binds = pts
            .iter()
            .map(|&p| {
                skin.iter()
                    .find_map(|t| {
                        let w = barycentric(p, dom(t[0]), dom(t[1]), dom(t[2]))?;
                        Some(Bind {
                            tri: t.map(usize::from),
                            w,
                        })
                    })
                    .expect("decal point on the face")
            })
            .collect();
        Self {
            binds,
            indices: tris.into_iter().flatten().collect(),
        }
    }

    /// Number of decal vertices.
    pub fn len(&self) -> usize {
        self.binds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.binds.is_empty()
    }

    /// Decal positions and normals on the skin given by `pos`/`normals`, lifted `lift` along
    /// the normal so the decal wins the depth test.
    pub fn evaluate(&self, pos: &[Vec3], normals: &[Vec3], lift: f32) -> Vec<(Vec3, Vec3)> {
        self.binds
            .iter()
            .map(|b| {
                let mix = |v: &[Vec3]| {
                    (0..3).fold([0.0; 3], |acc, k| add(acc, scale(v[b.tri[k]], b.w[k])))
                };
                let n = normalize(mix(normals));
                (add(mix(pos), scale(n, lift)), n)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{cross, dot, sub};
    use crate::rig::FaceRig;

    #[test]
    fn decals_bind_and_face_outward() {
        let rig = FaceRig::new();
        for region in [Region::Brows, Region::Lips] {
            let d = Decal::new(rig.topology(), region);
            let v = d.evaluate(rig.positions(), rig.normals(), 0.0);
            assert!(v.iter().all(|(p, _)| p.iter().all(|c| c.is_finite())));
            // Every non-degenerate triangle's winding agrees with the skin normal.
            for t in d.indices.as_chunks::<3>().0 {
                let [a, b, c] = t.map(|i| v[usize::from(i)]);
                let f = cross(sub(b.0, a.0), sub(c.0, a.0));
                if dot(f, f) > 1e-12 {
                    assert!(dot(f, a.1) > 0.0, "{region:?} triangle faces inward");
                }
            }
            // Symmetric: the decal's mean x is near 0.
            let mx: f32 = v.iter().map(|(p, _)| p[0]).sum::<f32>() / v.len() as f32;
            assert!(mx.abs() < 1e-3, "{region:?} mean x {mx}");
        }
    }

    #[test]
    fn lip_decals_part_with_the_lips() {
        let mut rig = FaceRig::new();
        let d = Decal::new(rig.topology(), Region::Lips);
        rig.set_params(&crate::FaceParams::default().with(crate::Param::JawOpen, 0.3));
        let v = d.evaluate(rig.positions(), rig.normals(), 0.0);
        let half = v.len() / 2;
        let upper_min = v[..half].iter().map(|(p, _)| p[1]).fold(f32::MAX, f32::min);
        let lower_max = v[half..].iter().map(|(p, _)| p[1]).fold(f32::MIN, f32::max);
        assert!(
            upper_min > lower_max,
            "lips overlap: {upper_min} vs {lower_max}"
        );
    }
}
