//! [`FaceRig`]: evaluates the parameter vector into a deformed mesh.
//!
//! Pipeline, per evaluation:
//! 1. `neutral(identity)` — rebuilt only when an identity parameter changes, together with the
//!    action-unit blend shapes and the jaw weights;
//! 2. `+ Σ auᵢ · Δᵢ` — linear expression blend shapes;
//! 3. linear blend skinning by the jaw and head bones;
//! 4. vertex normals.

use crate::landmarks::{self, LANDMARK_COUNT};
use crate::math::{Vec3, Xf, add, normalize, scale, sub};
use crate::muscles;
use crate::params::{ACTION_UNITS, FaceParams, Param};
use crate::shape::{self, Openings};
use crate::skeleton::{self, Bones};
use crate::topology::{Detail, Topology};

/// Rest data that depends on identity only.
struct Neutral {
    identity: Vec<f32>,
    pos: Vec<Vec3>,
    /// One delta per entry of [`ACTION_UNITS`].
    deltas: Vec<Vec<Vec3>>,
    eye_centers: [Vec3; 2],
    eye_radius: f32,
    mouth_center: Vec3,
    jaw_pivot: Vec3,
    neck_pivot: Vec3,
}

/// A procedural face: fixed topology, parameter-driven positions.
pub struct FaceRig {
    topo: Topology,
    jaw_weights: Vec<f32>,
    landmarks: Vec<u32>,
    landmark_triangles: Vec<[usize; 3]>,
    params: FaceParams,
    neutral: Neutral,
    bones: Bones,
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
}

impl Default for FaceRig {
    fn default() -> Self {
        Self::new()
    }
}

fn build_neutral(topo: &Topology, p: &FaceParams) -> Neutral {
    let open = shape::neutral_openings(p);
    let base = shape::sculpt(topo, p, open);
    // Muscles push along the normal. Each slit copy only sees its own lip's triangles, so its
    // normal tilts toward that lip; use the pair's average so pressing or bulging doesn't
    // pry closed lips apart.
    let mut normals = skeleton::normals(&base.pos, &topo.indices);
    for (i, &w) in topo.twin.iter().enumerate() {
        if (w as usize) > i {
            let n = normalize(add(normals[i], normals[w as usize]));
            normals[i] = n;
            normals[w as usize] = n;
        }
    }
    let diff = |other: Openings| {
        let s = shape::sculpt(topo, p, other);
        s.pos
            .iter()
            .zip(&base.pos)
            .map(|(a, b)| sub(*a, *b))
            .collect()
    };
    let deltas = ACTION_UNITS
        .iter()
        .map(|&au| {
            muscles::delta(au, topo, &base.pos, &normals).unwrap_or_else(|| match au {
                Param::UpperLidRaiser => diff(Openings {
                    lid_upper: open.lid_upper - 0.2,
                    ..open
                }),
                // Mostly the lower lid rising: combined with AU5 it gives a glare.
                Param::LidTightener => diff(Openings {
                    lid_upper: open.lid_upper * 0.95,
                    lid_lower: open.lid_lower * 0.1 - 0.06,
                    ..open
                }),
                Param::EyesClosed => {
                    // Both lids rotate to the same line, slightly below the middle.
                    let meet = 0.3 * open.lid_lower;
                    diff(Openings {
                        lid_upper: meet,
                        lid_lower: meet,
                        ..open
                    })
                }
                Param::LipsPart => diff(Openings { lips: 1.0, ..open }),
                other => unreachable!("{other:?} has neither muscles nor an opening"),
            })
        })
        .collect();
    Neutral {
        identity: p.identity().to_vec(),
        pos: base.pos,
        deltas,
        eye_centers: base.eye_centers,
        eye_radius: base.eye_radius,
        mouth_center: base.mouth_center,
        jaw_pivot: base.jaw_pivot,
        neck_pivot: base.neck_pivot,
    }
}

impl FaceRig {
    /// Build the rig at default parameters and standard resolution.
    pub fn new() -> Self {
        Self::with_detail(Detail::Standard)
    }

    /// Build the rig at default parameters on a mesh of the given resolution.
    pub fn with_detail(detail: Detail) -> Self {
        let topo = Topology::with_detail(detail);
        let params = FaceParams::default();
        let neutral = build_neutral(&topo, &params);
        let mut rig = Self {
            jaw_weights: skeleton::jaw_weights(&topo),
            landmarks: landmarks::resolve(&topo),
            landmark_triangles: Vec::new(),
            bones: skeleton::pose(&params, neutral.jaw_pivot, neutral.neck_pivot),
            positions: Vec::new(),
            normals: Vec::new(),
            topo,
            params,
            neutral,
        };
        // Triangulate the landmarks once, frontally, with the lips apart so the inner-lip
        // points (which coincide when the mouth is closed) are distinct.
        rig.set_params(
            &FaceParams::default()
                .with(Param::LipsPart, 1.0)
                .with(Param::JawOpen, 0.1),
        );
        let xy = rig.landmarks().map(|p| [p[0], p[1]]);
        rig.landmark_triangles = landmarks::delaunay(&xy);
        rig.set_params(&FaceParams::default());
        rig
    }

    /// Current parameters.
    pub fn params(&self) -> &FaceParams {
        &self.params
    }

    /// Set the parameters and re-evaluate the mesh. Identity changes rebuild the neutral and
    /// blend shapes (a few milliseconds); expression and pose changes are cheap.
    pub fn set_params(&mut self, params: &FaceParams) {
        self.params = params.clone();
        self.params.clamp();
        if self.params.identity() != self.neutral.identity.as_slice() {
            self.neutral = build_neutral(&self.topo, &self.params);
        }
        self.evaluate();
    }

    fn evaluate(&mut self) {
        let n = &self.neutral;
        let weights: Vec<(usize, f32)> = ACTION_UNITS
            .iter()
            .enumerate()
            .map(|(k, &au)| (k, self.params.get(au)))
            .filter(|&(_, w)| w != 0.0)
            .collect();
        self.bones = skeleton::pose(&self.params, n.jaw_pivot, n.neck_pivot);
        self.positions = n
            .pos
            .iter()
            .enumerate()
            .map(|(i, &p)| {
                let p = weights
                    .iter()
                    .fold(p, |p, &(k, w)| add(p, scale(n.deltas[k][i], w)));
                skeleton::skin(&self.bones, p, self.jaw_weights[i])
            })
            .collect();
        self.normals = skeleton::normals(&self.positions, &self.topo.indices);
    }

    /// Deformed vertex positions, head units (about 1 = half the head height).
    pub fn positions(&self) -> &[Vec3] {
        &self.positions
    }

    pub fn normals(&self) -> &[Vec3] {
        &self.normals
    }

    /// Triangle list, counter-clockwise from outside.
    pub fn indices(&self) -> &[u16] {
        &self.topo.indices
    }

    /// The fixed connectivity and domain coordinates.
    pub fn topology(&self) -> &Topology {
        &self.topo
    }

    /// Vertex ids of the 68 landmarks (see [`crate::landmarks`]).
    pub fn landmark_vertices(&self) -> &[u32] {
        &self.landmarks
    }

    /// Current positions of the 68 landmarks.
    pub fn landmarks(&self) -> [Vec3; LANDMARK_COUNT] {
        std::array::from_fn(|i| self.positions[self.landmarks[i] as usize])
    }

    /// Fixed triangulation of the 68 landmarks (Delaunay on the frontal face), as indices into
    /// [`FaceRig::landmarks`]. The same triangles apply to any pose or expression, which is what
    /// piecewise-affine warping between a photo and the rig needs.
    pub fn landmark_triangles(&self) -> &[[usize; 3]] {
        &self.landmark_triangles
    }

    /// Rigid transforms of the right (`-x`) and left eyeball: origin at the eyeball centre,
    /// `+z` the gaze direction. Scale by [`FaceRig::eye_radius`] to draw a unit sphere.
    pub fn eye_transforms(&self) -> [Xf; 2] {
        self.neutral.eye_centers.map(|c| {
            self.bones
                .head
                .compose(&Xf::translation(c))
                .compose(&self.bones.eye_rot)
        })
    }

    /// Eye yaw and pitch ([`Param::EyeYaw`], [`Param::EyePitch`]; radians, head space) that
    /// aim the gaze from between the eyes at world point `target`, with the head posed as in
    /// `params`. Not clamped to the parameter ranges.
    pub fn gaze_toward(&self, params: &FaceParams, target: Vec3) -> [f32; 2] {
        let head = skeleton::pose(params, self.neutral.jaw_pivot, self.neutral.neck_pivot).head;
        let [r, l] = self.neutral.eye_centers;
        let mid = scale(add(r, l), 0.5);
        let d = normalize(sub(head.inverse().point(target), mid));
        // Inverse of the eye bone's R_y(yaw) · R_x(pitch) applied to +z.
        [d[0].atan2(d[2]), (-d[1]).clamp(-1.0, 1.0).asin()]
    }

    pub fn eye_radius(&self) -> f32 {
        self.neutral.eye_radius
    }

    /// Transform and radii of an ellipsoid that darkens the inside of the mouth. It follows
    /// the jaw halfway so it stays behind both lips.
    pub fn mouth_cavity(&self) -> (Xf, Vec3) {
        let half = skeleton::pose(
            &self
                .params
                .clone()
                .with(Param::JawOpen, 0.5 * self.params.get(Param::JawOpen))
                .with(Param::JawSide, 0.5 * self.params.get(Param::JawSide)),
            self.neutral.jaw_pivot,
            self.neutral.neck_pivot,
        );
        let c = sub(self.neutral.mouth_center, [0.0, 0.03, 0.2]);
        let xf = half.head.compose(&half.jaw).compose(&Xf::translation(c));
        (xf, [0.2, 0.13, 0.16])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::length;
    use crate::params::PARAMS;
    use crate::topology::Side;

    fn max_dist(a: &[Vec3], b: &[Vec3]) -> f32 {
        a.iter()
            .zip(b)
            .map(|(p, q)| length(sub(*p, *q)))
            .fold(0.0, f32::max)
    }

    fn assert_symmetric(rig: &FaceRig, what: &str) {
        let pos = rig.positions();
        for (i, &m) in rig.topology().mirror.iter().enumerate() {
            let (p, q) = (pos[i], pos[m as usize]);
            let err = (p[0] + q[0]).abs() + (p[1] - q[1]).abs() + (p[2] - q[2]).abs();
            assert!(err < 1e-4, "{what}: vertex {i} vs {m}: {p:?} {q:?}");
        }
    }

    #[test]
    fn default_rig_is_finite_and_symmetric() {
        let rig = FaceRig::new();
        assert!(rig.positions().iter().flatten().all(|v| v.is_finite()));
        assert!(rig.normals().iter().flatten().all(|v| v.is_finite()));
        assert_symmetric(&rig, "neutral");
        // The face looks down +z: the nose tip is the frontmost landmark region.
        let lm = rig.landmarks();
        assert!(lm[30][2] > lm[39][2] && lm[30][2] > lm[51][2]);
        assert!(lm.iter().flatten().all(|v| v.is_finite()));
    }

    #[test]
    fn every_action_unit_is_symmetric_local_and_moves_something() {
        let mut rig = FaceRig::new();
        let neutral = rig.positions().to_vec();
        for &au in ACTION_UNITS {
            rig.set_params(&FaceParams::default().with(au, 1.0));
            let moved = max_dist(rig.positions(), &neutral);
            assert!(moved > 0.005, "{au:?} moves only {moved}");
            assert!(moved < 0.15, "{au:?} moves {moved}");
            assert_symmetric(&rig, &format!("{au:?}"));
            // The back of the head never moves.
            for (v, (p, q)) in rig
                .topology()
                .vertices
                .iter()
                .zip(rig.positions().iter().zip(&neutral))
            {
                if v.dom[0].abs() > 2.0 {
                    assert!(
                        length(sub(*p, *q)) < 1e-4,
                        "{au:?} moves the back of the head"
                    );
                }
            }
        }
        rig.set_params(&FaceParams::default());
        assert_eq!(rig.positions(), neutral.as_slice());
    }

    #[test]
    fn jaw_opens_the_mouth_and_leaves_the_forehead() {
        let mut rig = FaceRig::new();
        let neutral = rig.positions().to_vec();
        let gap = |rig: &FaceRig| length(sub(rig.landmarks()[62], rig.landmarks()[66]));
        assert!(gap(&rig) < 1e-5, "lips touch at rest");
        rig.set_params(&FaceParams::default().with(Param::JawOpen, 0.3));
        assert!(gap(&rig) > 0.1, "jaw opens the lips: {}", gap(&rig));
        for (v, (p, q)) in rig
            .topology()
            .vertices
            .iter()
            .zip(rig.positions().iter().zip(&neutral))
        {
            if v.dom[1] > 0.3 {
                assert!(length(sub(*p, *q)) < 1e-5);
            }
            if v.mouth == Side::Upper {
                assert!(length(sub(*p, *q)) < 1e-5, "upper lip stays");
            }
        }
    }

    #[test]
    fn lip_tightener_and_pucker_keep_lips_closed() {
        let mut rig = FaceRig::new();
        for au in [Param::LipTightener, Param::LipPucker, Param::ChinRaiser] {
            rig.set_params(&FaceParams::default().with(au, 1.0));
            let gap = length(sub(rig.landmarks()[62], rig.landmarks()[66]));
            assert!(gap < 0.01, "{au:?} parts the lips by {gap}");
        }
    }

    #[test]
    fn landmark_triangulation_covers_every_landmark() {
        let rig = FaceRig::new();
        let tris = rig.landmark_triangles();
        // A triangulation of n points with h on the hull has 2n - 2 - h triangles.
        assert!(
            (100..=132).contains(&tris.len()),
            "{} triangles",
            tris.len()
        );
        for i in 0..LANDMARK_COUNT {
            assert!(
                tris.iter().flatten().any(|&j| j == i),
                "landmark {i} unused"
            );
        }
    }

    #[test]
    fn gaze_toward_aims_the_eyes_for_any_head_pose() {
        let mut rig = FaceRig::new();
        let target = [0.6, 0.4, 3.0];
        let mut p = FaceParams::default()
            .with(Param::HeadYaw, 0.4)
            .with(Param::HeadPitch, -0.3)
            .with(Param::HeadRoll, 0.35);
        let [yaw, pitch] = rig.gaze_toward(&p, target);
        p.set(Param::EyeYaw, yaw);
        p.set(Param::EyePitch, pitch);
        rig.set_params(&p);
        let [r, l] = rig.eye_transforms();
        let mid = scale(add(r.t, l.t), 0.5);
        let want = normalize(sub(target, mid));
        let got = r.vector([0.0, 0.0, 1.0]);
        assert!(crate::math::dot(want, got) > 0.9999, "{want:?} vs {got:?}");
    }

    #[test]
    fn low_detail_rig_still_works() {
        let mut rig = FaceRig::with_detail(Detail::Low);
        assert!(rig.positions().iter().flatten().all(|v| v.is_finite()));
        assert_symmetric(&rig, "low neutral");
        let lm =
            |rig: &FaceRig, a: usize, b: usize| length(sub(rig.landmarks()[a], rig.landmarks()[b]));
        assert!(lm(&rig, 37, 41) > 0.02, "eyes open");
        rig.set_params(&FaceParams::default().with(Param::EyesClosed, 1.0));
        assert!(lm(&rig, 37, 41) < 0.005, "eyes close");
        rig.set_params(&FaceParams::default().with(Param::JawOpen, 0.3));
        assert!(lm(&rig, 62, 66) > 0.1, "jaw opens the lips");
        assert!(rig.landmark_triangles().len() >= 100);
    }

    #[test]
    fn eyes_open_at_rest_and_close_with_au43() {
        let mut rig = FaceRig::new();
        let gap = |rig: &FaceRig| length(sub(rig.landmarks()[37], rig.landmarks()[41]));
        assert!(gap(&rig) > 0.03, "eye open: {}", gap(&rig));
        rig.set_params(&FaceParams::default().with(Param::EyesClosed, 1.0));
        assert!(gap(&rig) < 0.005, "eye closed: {}", gap(&rig));
    }

    #[test]
    fn identity_extremes_stay_finite_and_symmetric() {
        let mut rig = FaceRig::new();
        for s in PARAMS
            .iter()
            .filter(|s| s.group == crate::params::Group::Identity)
        {
            for v in [s.min, s.max] {
                rig.set_params(&FaceParams::default().with(s.param, v));
                assert!(rig.positions().iter().flatten().all(|v| v.is_finite()));
                assert_symmetric(&rig, s.name);
            }
        }
    }

    #[test]
    fn landmarks_mirror() {
        let rig = FaceRig::new();
        let lm = rig.landmarks();
        for (i, &m) in landmarks::MIRROR.iter().enumerate() {
            let (p, q) = (lm[i], lm[m]);
            assert!(
                (p[0] + q[0]).abs() < 1e-4 && (p[1] - q[1]).abs() < 1e-4,
                "{i} vs {m}"
            );
        }
    }

    #[test]
    fn head_pose_is_rigid() {
        let mut rig = FaceRig::new();
        let a = rig.landmarks();
        rig.set_params(&FaceParams::default().with(Param::HeadYaw, 0.5));
        let b = rig.landmarks();
        let d = |l: &[Vec3; LANDMARK_COUNT]| length(sub(l[36], l[45]));
        assert!((d(&a) - d(&b)).abs() < 1e-5);
        assert!(b[30][0] > a[30][0], "yaw turns the nose toward +x");
    }
}
