//! Identity sculpt: turns the canonical domain into a neutral head for a given set of identity
//! parameters. A warped ellipsoid carries smooth height features (nose, brows, cheeks, lips,
//! chin, ears), the skin around each eye wraps onto a sphere around the eyeball, and the lids
//! and lips open by rotating or sliding the vertices next to their cut lines.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::math::{Vec3, Xf, add, gauss2, lerp, normalize, scale, smoothstep, sub};
use crate::params::{FaceParams, Param};
use crate::topology::{Layout, Side, Topology};

/// Lid and lip openings the sculpt is evaluated at. Lid angles rotate about the eyeball's
/// horizontal axis; positive moves the lid down.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Openings {
    pub lid_upper: f32,
    pub lid_lower: f32,
    /// `0` = lips touching, `1` = parted (AU25).
    pub lips: f32,
}

/// A sculpted head plus the rest positions of the rig's anchors.
pub(crate) struct Sculpt {
    pub pos: Vec<Vec3>,
    /// Right (`-x`) then left (`+x`) eyeball centre.
    pub eye_centers: [Vec3; 2],
    pub eye_radius: f32,
    /// Surface point at the centre of the mouth line.
    pub mouth_center: Vec3,
    pub jaw_pivot: Vec3,
    pub neck_pivot: Vec3,
}

/// Lid opening of the neutral face for these identity parameters.
pub(crate) fn neutral_openings(p: &FaceParams) -> Openings {
    let a = p.get(Param::EyeAperture);
    Openings {
        lid_upper: -(0.45 + 0.15 * a),
        lid_lower: 0.18 + 0.05 * a,
        lips: 0.0,
    }
}

struct Shape<'a> {
    p: &'a FaceParams,
    l: Layout,
    radii: Vec3,
}

impl Shape<'_> {
    fn id(&self, param: Param) -> f32 {
        self.p.get(param)
    }

    /// Point on the head (before eye wrapping) for canonical domain `(x, y)`.
    fn point(&self, x: f32, y: f32) -> Vec3 {
        let l = &self.l;
        // Domain warps: move whole feature regions (eyes, mouth, chin) without changing
        // connectivity.
        let (mut xw, mut yw) = (x, y);
        for s in [-1.0f32, 1.0] {
            let ex = s * l.eye_x();
            let g = gauss2(x - ex, y - l.eye_y, 0.22, 0.2);
            xw += s * 0.05 * self.id(Param::EyeSpacing) * g
                + (x - ex) * 0.15 * self.id(Param::EyeSize) * g;
            yw += (y - l.eye_y) * 0.15 * self.id(Param::EyeSize) * g;
        }
        xw += x * 0.2 * self.id(Param::MouthWidth) * gauss2(x, y - l.mouth_y, 0.38, 0.18);
        // The mouth slit is a latitude ring, which below eye level reads as a smile. Drop the
        // corners so the neutral mouth line looks straight from the front.
        let mw = l.mouth_half_w;
        yw -= 0.06 * (x / mw).powi(2) * gauss2(x, y - l.mouth_y, 1.6 * mw, 0.2);
        yw -= 0.1
            * self.id(Param::ChinLength)
            * smoothstep(l.mouth_y, -1.15, y)
            * (1.0 - smoothstep(-1.2, -1.5, y));

        let d = [yw.cos() * xw.sin(), yw.sin(), yw.cos() * xw.cos()];
        let [rx, ry, rz] = self.radii;
        // The ellipsoid's lower half is too narrow and too shallow for a jaw: widen it into a
        // U and bring the front of the lower face forward. Widen the cranium above the brows.
        let lower = smoothstep(-0.4, -1.1, y) * (1.0 - smoothstep(-1.25, -1.5, y));
        let wide =
            1.0 + (0.18 + 0.1 * self.id(Param::JawWidth)) * lower + 0.08 * smoothstep(0.0, 0.7, y);
        let deep = 1.0 + 0.25 * smoothstep(0.0, -0.9, y) * d[2].max(0.0)
            - 0.12 * smoothstep(0.0, -0.6, d[2]);
        let e = [rx * d[0] * wide, ry * d[1], rz * d[2] * deep];
        let n = normalize([d[0] / rx, d[1] / ry, d[2] / rz]);
        add(e, scale(n, self.height(x, y)))
    }

    /// Height of the surface features above the ellipsoid, in head units.
    fn height(&self, x: f32, y: f32) -> f32 {
        let l = &self.l;
        let ax = x.abs();
        let (my, mw) = (l.mouth_y, l.mouth_half_w);
        let mut h = 0.0;

        // Lower face: the mandible fills out the ellipsoid below the cheeks.
        h += 0.18
            * smoothstep(-0.3, -1.0, y)
            * (1.0 - smoothstep(-1.05, -1.4, y))
            * gauss2(x, 0.0, 0.55, 1.0);

        // Nose: a ridge growing from the bridge to the tip, plus the alae.
        let nl = self.id(Param::NoseLength);
        let wn = 1.0 + 0.35 * self.id(Param::NoseWidth);
        let (bridge, tip) = (0.06, -0.27 - 0.05 * nl);
        let h_tip = 0.17 * (1.0 + 0.35 * self.id(Param::NoseProtrusion));
        let ridge = if y > bridge {
            0.02 * gauss2(0.0, y - bridge, 1.0, 0.08)
        } else if y > tip {
            let t = (bridge - y) / (bridge - tip);
            0.02 + (h_tip - 0.02) * t.powf(1.5)
        } else {
            h_tip * gauss2(0.0, y - tip, 1.0, 0.06)
        };
        let t_w = ((bridge - y) / (bridge - tip)).clamp(0.0, 1.0);
        let sx = (0.045 + 0.035 * t_w) * wn;
        h += ridge * gauss2(x, 0.0, sx, 1.0);
        h += 0.06 * gauss2(ax - 0.085 * wn, y - (tip - 0.05), 0.045 * wn, 0.045);

        // Brow ridge and cheekbones.
        h += 0.035
            * (1.0 + 0.6 * self.id(Param::BrowRidge))
            * gauss2(0.0, y - 0.17, 1.0, 0.07)
            * (1.0 - smoothstep(0.45, 0.65, ax));
        h += 0.035
            * (1.0 + 0.8 * self.id(Param::Cheekbones))
            * gauss2(ax - 0.45, y + 0.2, 0.14, 0.1);

        // Chin and the groove above it.
        h += (0.05 + 0.04 * self.id(Param::ChinProtrusion)) * gauss2(x, y + 0.98, 0.26, 0.13);
        h -= 0.015 * gauss2(x, y - (my - 0.17), 0.15, 0.04);

        // Lips, the line where they meet, and the inset corners.
        let lf = 1.0 + 0.6 * self.id(Param::LipFullness);
        h += 0.035 * lf * gauss2(x, y - (my + 0.045), 0.2, 0.045);
        h += 0.04 * lf * gauss2(x, y - (my - 0.06), 0.17, 0.05);
        h -= 0.01 * gauss2(x, y - my, mw, 0.015);
        h -= 0.02 * gauss2(ax - mw, y - my, 0.05, 0.05);

        // Ears.
        h += 0.07 * gauss2(ax - FRAC_PI_2, y + 0.05, 0.08, 0.2);
        h
    }
}

/// Sculpt every vertex of `topo` for the identity in `p` at the given openings.
pub(crate) fn sculpt(topo: &Topology, p: &FaceParams, open: Openings) -> Sculpt {
    let l = topo.layout;
    let radii = [
        0.78 * (1.0 + 0.12 * p.get(Param::HeadWidth)),
        1.0 + 0.1 * p.get(Param::HeadHeight),
        0.92 * (1.0 + 0.1 * p.get(Param::HeadDepth)),
    ];
    let shape = Shape { p, l, radii };

    let eye_radius = 0.118 * (1.0 + 0.12 * p.get(Param::EyeSize));
    let lid_radius = eye_radius * 1.07;
    let inset = 0.035;
    let eye_centers = [-1.0f32, 1.0].map(|s| {
        let pe = shape.point(s * l.eye_x(), l.eye_y);
        sub(pe, [0.0, 0.0, lid_radius + inset])
    });

    let (ey, inner, outer) = (l.eye_y, l.eye_inner, l.eye_outer);
    let (my, mw) = (l.mouth_y, l.mouth_half_w);
    let pos = topo
        .vertices
        .iter()
        .map(|v| {
            let [x, y] = v.dom;
            let mut q = shape.point(x, y);
            let ci = usize::from(x > 0.0);
            let c = eye_centers[ci];

            // Wrap the skin around the eyeball so the lids slide over it.
            let d = (((x.abs() - l.eye_x()) / 0.21).powi(2) + ((y - ey) / 0.15).powi(2)).sqrt();
            let w = 1.0 - smoothstep(0.6, 1.1, d);
            if w > 0.0 {
                let on_sphere = add(c, scale(normalize(sub(q, c)), lid_radius));
                q = lerp(q, on_sphere, w);
            }

            // Lids: rotate about the eyeball's horizontal axis, tapering to the corners.
            let t = (x.abs() - inner) / (outer - inner);
            if (0.0..=1.0).contains(&t) {
                let prof = (PI * t).sin().max(0.0).powf(0.8);
                let fall = 1.0 - smoothstep(0.0, 0.14, (y - ey).abs());
                let upper = match v.eye {
                    Side::Upper => true,
                    Side::Lower => false,
                    Side::None => y > ey,
                };
                let a = if upper {
                    open.lid_upper
                } else {
                    open.lid_lower
                };
                if a != 0.0 && fall > 0.0 {
                    q = Xf::rot_x(a * prof * fall).about(c).point(q);
                }
            }

            // Lips part along the mouth line.
            let t = (x + mw) / (2.0 * mw);
            if open.lips != 0.0 && (0.0..=1.0).contains(&t) {
                let prof = (PI * t).sin().max(0.0).powf(0.7);
                let fall = 1.0 - smoothstep(0.0, 0.12, (y - my).abs());
                let upper = match v.mouth {
                    Side::Upper => true,
                    Side::Lower => false,
                    Side::None => y > my,
                };
                let dy = if upper { 0.018 } else { -0.03 };
                q[1] += dy * open.lips * prof * fall;
            }
            q
        })
        .collect();

    let [_, ry, rz] = radii;
    Sculpt {
        pos,
        eye_centers,
        eye_radius,
        mouth_center: shape.point(0.0, my),
        jaw_pivot: [0.0, -0.2 * ry, -0.1 * rz],
        neck_pivot: [0.0, -0.9 * ry, -0.15 * rz],
    }
}
