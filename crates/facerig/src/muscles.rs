//! FACS action units as Waters-style muscles, baked into per-vertex delta blend shapes.
//!
//! A linear muscle is anchored at an origin (fixed to bone) and pulls the skin around its
//! insertion toward that origin, sliding along the surface. A sphincter muscle contracts the
//! skin of an elliptical ring toward its centre. Both are evaluated once per identity at full
//! activation; at runtime an action unit is just `weight * delta`, which keeps expressions
//! linear in their parameters (what a fitter wants).

use crate::math::{Vec3, add, dot, gauss2, normalize, scale, smoothstep, sub};
use crate::params::Param;
use crate::topology::{Layout, Side, Topology, VertexInfo};

/// Which lip a muscle may move.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Lip {
    Both,
    Upper,
}

#[derive(Copy, Clone, Debug)]
enum Kind {
    Linear {
        origin: [f32; 2],
        insertion: [f32; 2],
        /// Influence radius around the insertion (domain units).
        radius: [f32; 2],
    },
    Sphincter {
        center: [f32; 2],
        radius: [f32; 2],
        /// Push along the surface normal (protrusion, or pressing with a negative value).
        forward: f32,
    },
}

#[derive(Copy, Clone, Debug)]
struct Muscle {
    kind: Kind,
    /// Surface slide at full activation, head units.
    pull: f32,
    /// Bulge along the normal at full activation (tissue bunching).
    bulge: f32,
    lip: Lip,
    /// Defined on the face's right (`-X`); also add the mirrored copy.
    mirror: bool,
}

fn linear(origin: [f32; 2], insertion: [f32; 2], radius: [f32; 2], pull: f32) -> Muscle {
    Muscle {
        kind: Kind::Linear {
            origin,
            insertion,
            radius,
        },
        pull,
        bulge: 0.0,
        lip: Lip::Both,
        mirror: true,
    }
}

impl Muscle {
    fn bulge(self, b: f32) -> Self {
        Self { bulge: b, ..self }
    }
    fn lip(self, lip: Lip) -> Self {
        Self { lip, ..self }
    }
    fn center(self) -> Self {
        Self {
            mirror: false,
            ..self
        }
    }
}

/// The muscles behind `au`, or `None` for action units the sculpt produces procedurally
/// (lid and lip openings).
fn muscles(au: Param, l: &Layout) -> Option<Vec<Muscle>> {
    let (ex, ey) = (l.eye_x(), l.eye_y);
    let (my, mw) = (l.mouth_y, l.mouth_half_w);
    let sphincter = |radius: [f32; 2], pull: f32, forward: f32| Muscle {
        kind: Kind::Sphincter {
            center: [0.0, my],
            radius,
            forward,
        },
        pull,
        bulge: 0.0,
        lip: Lip::Both,
        mirror: false,
    };
    Some(match au {
        Param::InnerBrowRaiser => vec![linear([-0.12, 0.85], [-0.12, 0.22], [0.12, 0.14], 0.05)],
        Param::OuterBrowRaiser => vec![linear([-0.45, 0.85], [-0.45, 0.22], [0.14, 0.14], 0.045)],
        Param::BrowLowerer => vec![
            // Corrugator: drags the inner brow down and toward the nose, bunching it up.
            linear([-0.03, 0.02], [-0.2, 0.22], [0.16, 0.11], 0.09).bulge(0.02),
            // Procerus: pulls the skin between the brows down.
            linear([0.0, -0.05], [0.0, 0.2], [0.09, 0.12], 0.04).center(),
        ],
        Param::CheekRaiser => vec![
            linear(
                [-ex - 0.05, ey + 0.02],
                [-ex - 0.02, ey - 0.2],
                [0.15, 0.1],
                0.03,
            )
            .bulge(0.012),
        ],
        Param::NoseWrinkler => {
            vec![linear([-0.06, 0.08], [-0.09, -0.33], [0.06, 0.1], 0.025).bulge(0.005)]
        }
        Param::UpperLipRaiser => {
            vec![linear([-0.17, -0.2], [-0.1, my + 0.04], [0.09, 0.07], 0.03).lip(Lip::Upper)]
        }
        Param::LipCornerPuller => {
            vec![linear([-0.7, -0.12], [-mw, my], [0.12, 0.1], 0.06).bulge(0.01)]
        }
        Param::LipCornerDepressor => vec![linear([-0.35, -0.95], [-mw, my], [0.1, 0.09], 0.04)],
        Param::ChinRaiser => vec![
            linear([0.0, -1.0], [0.0, my - 0.1], [0.15, 0.12], 0.025)
                .bulge(0.02)
                .center(),
        ],
        Param::LipPucker => vec![sphincter([mw * 1.1, 0.13], 0.05, 0.04)],
        Param::LipStretcher => vec![linear([-0.85, my - 0.05], [-mw, my], [0.1, 0.08], 0.04)],
        Param::LipTightener => vec![sphincter([mw, 0.1], 0.025, -0.012)],
        _ => return None,
    })
}

/// `1` above the mouth line, `0` below, soft in between; exact on the slit copies.
fn upper_lip_mask(v: &VertexInfo, my: f32) -> f32 {
    match v.mouth {
        Side::Upper => 1.0,
        Side::Lower => 0.0,
        Side::None => smoothstep(-0.02, 0.02, v.dom[1] - my),
    }
}

fn nearest(topo: &Topology, dom: [f32; 2]) -> usize {
    topo.vertices
        .iter()
        .enumerate()
        .filter(|(_, v)| v.mouth != Side::Lower && v.eye != Side::Lower)
        .min_by(|(_, a), (_, b)| {
            let da = (a.dom[0] - dom[0]).powi(2) + (a.dom[1] - dom[1]).powi(2);
            let db = (b.dom[0] - dom[0]).powi(2) + (b.dom[1] - dom[1]).powi(2);
            da.total_cmp(&db)
        })
        .map(|(i, _)| i)
        .expect("vertices")
}

fn mirrored(m: Muscle) -> Muscle {
    let f = |p: [f32; 2]| [-p[0], p[1]];
    let kind = match m.kind {
        Kind::Linear {
            origin,
            insertion,
            radius,
        } => Kind::Linear {
            origin: f(origin),
            insertion: f(insertion),
            radius,
        },
        k @ Kind::Sphincter { .. } => k,
    };
    Muscle { kind, ..m }
}

/// Delta blend shape of `au` at full activation on the neutral `pos`/`normals`, or `None` if
/// the action unit is procedural.
pub(crate) fn delta(
    au: Param,
    topo: &Topology,
    pos: &[Vec3],
    normals: &[Vec3],
) -> Option<Vec<Vec3>> {
    let l = topo.layout;
    let list = muscles(au, &l)?;
    let mut out = vec![[0.0; 3]; pos.len()];
    for m in list
        .iter()
        .flat_map(|&m| std::iter::once(m).chain(m.mirror.then(|| mirrored(m))))
    {
        let toward = match m.kind {
            Kind::Linear { origin, .. } => pos[nearest(topo, origin)],
            Kind::Sphincter { center, .. } => pos[nearest(topo, center)],
        };
        for (i, v) in topo.vertices.iter().enumerate() {
            let [x, y] = v.dom;
            let (w, fwd) = match m.kind {
                Kind::Linear {
                    origin,
                    insertion,
                    radius,
                } => {
                    let a = [insertion[0] - origin[0], insertion[1] - origin[1]];
                    let s = ((x - origin[0]) * a[0] + (y - origin[1]) * a[1])
                        / (a[0] * a[0] + a[1] * a[1]);
                    let w = gauss2(x - insertion[0], y - insertion[1], radius[0], radius[1])
                        * smoothstep(0.15, 0.6, s);
                    (w, 0.0)
                }
                Kind::Sphincter {
                    center,
                    radius,
                    forward,
                } => {
                    let r2 = ((x - center[0]) / radius[0]).powi(2)
                        + ((y - center[1]) / radius[1]).powi(2);
                    // Ring weight: zero at the centre, peaking near the lips' edge.
                    let ring = 2.33 * r2.sqrt() * (-r2).exp();
                    (ring, forward * (-r2).exp())
                }
            };
            let mask = match m.lip {
                Lip::Both => 1.0,
                Lip::Upper => upper_lip_mask(v, l.mouth_y),
            };
            let w = w * mask;
            if w < 1e-5 && fwd.abs() < 1e-5 {
                continue;
            }
            let n = normals[i];
            let d = sub(toward, pos[i]);
            let tangent = normalize(sub(d, scale(n, dot(n, d))));
            let disp = add(
                scale(tangent, m.pull * w),
                scale(n, m.bulge * w + fwd * mask),
            );
            out[i] = add(out[i], disp);
        }
    }
    Some(out)
}
