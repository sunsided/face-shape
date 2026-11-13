//! The 68 facial landmarks in the iBUG 300-W layout, pinned to mesh vertices.
//!
//! | Indices | Feature |
//! |---------|---------|
//! | 0–16 | jaw line, face's right to left |
//! | 17–21, 22–26 | right and left brow |
//! | 27–30, 31–35 | nose bridge, nostril base |
//! | 36–41, 42–47 | right and left eye |
//! | 48–59, 60–67 | outer and inner lip contour |
//!
//! "Right" is the face's own right, at `-x` (image left when the face looks at the camera).
//! Landmarks are fixed vertex ids, so they follow every identity, expression and pose change;
//! this is the bridge to fitting the rig to 2D landmarks detected in an image.

use std::f32::consts::PI;

use crate::topology::{Side, Topology};

/// Number of landmarks.
pub const LANDMARK_COUNT: usize = 68;

/// Brow landmarks as `(|X|, Y)` in the domain, outer to inner. Also the centre line of the
/// brow [`Region`](crate::regions::Region).
pub(crate) const BROW: [(f32, f32); 5] = [
    (0.50, 0.17),
    (0.42, 0.22),
    (0.33, 0.245),
    (0.24, 0.235),
    (0.14, 0.21),
];

#[derive(Copy, Clone)]
enum On {
    Skin,
    EyeUpper,
    EyeLower,
    MouthUpper,
    MouthLower,
}

/// Domain coordinates (and slit side) of every landmark.
fn targets(topo: &Topology) -> Vec<([f32; 2], On)> {
    let l = topo.layout;
    let mut t = Vec::with_capacity(LANDMARK_COUNT);

    // Jaw: a U from ear level down around the chin.
    for k in 0..17 {
        let a = PI * k as f32 / 16.0;
        t.push(([-a.cos(), -0.05 - 0.95 * a.sin()], On::Skin));
    }
    // Brows, each listed from the face's right to left.
    t.extend(BROW.iter().map(|&(x, y)| ([-x, y], On::Skin)));
    t.extend(BROW.iter().rev().map(|&(x, y)| ([x, y], On::Skin)));
    // Nose bridge to tip, then the nostril base.
    t.extend([0.06, -0.06, -0.17, -0.28].map(|y| ([0.0, y], On::Skin)));
    t.extend(
        [
            (-0.11, -0.37),
            (-0.055, -0.39),
            (0.0, -0.4),
            (0.055, -0.39),
            (0.11, -0.37),
        ]
        .map(|(x, y)| ([x, y], On::Skin)),
    );
    // Eyes: corner, two upper-lid points, corner, two lower-lid points (clockwise in the image).
    let (ey, inner, outer) = (l.eye_y, l.eye_inner, l.eye_outer);
    let at = |f: f32| outer + (inner - outer) * f; // |X| from outer (0) to inner (1) corner
    let right = [
        (at(0.0), On::Skin),
        (at(1.0 / 3.0), On::EyeUpper),
        (at(2.0 / 3.0), On::EyeUpper),
        (at(1.0), On::Skin),
        (at(2.0 / 3.0), On::EyeLower),
        (at(1.0 / 3.0), On::EyeLower),
    ];
    t.extend(right.iter().map(|&(x, on)| ([-x, ey], on)));
    let left = [
        (at(1.0), On::Skin),
        (at(2.0 / 3.0), On::EyeUpper),
        (at(1.0 / 3.0), On::EyeUpper),
        (at(0.0), On::Skin),
        (at(1.0 / 3.0), On::EyeLower),
        (at(2.0 / 3.0), On::EyeLower),
    ];
    t.extend(left.iter().map(|&(x, on)| ([x, ey], on)));
    let (my, mw) = (l.mouth_y, l.mouth_half_w);
    // Outer lips, on the vermilion border: right corner, upper lip to the left corner, lower
    // lip back.
    let up = |x: f32| ([x, my + crate::regions::lip_outline(&l, x).0], On::Skin);
    let down = |x: f32| ([x, my - crate::regions::lip_outline(&l, x).1], On::Skin);
    t.extend([
        ([-mw, my], On::Skin),
        up(-0.16),
        up(-0.06),
        up(0.0),
        up(0.06),
        up(0.16),
        ([mw, my], On::Skin),
        down(0.16),
        down(0.07),
        down(0.0),
        down(-0.07),
        down(-0.16),
    ]);
    // Inner lips, on the mouth slit.
    let c = mw - 0.03;
    let inner_lip = [
        (-c, On::MouthUpper),
        (-0.08, On::MouthUpper),
        (0.0, On::MouthUpper),
        (0.08, On::MouthUpper),
        (c, On::MouthUpper),
        (0.08, On::MouthLower),
        (0.0, On::MouthLower),
        (-0.08, On::MouthLower),
    ];
    t.extend(inner_lip.map(|(x, on)| ([x, my], on)));
    debug_assert_eq!(t.len(), LANDMARK_COUNT);
    t
}

/// Resolve the landmarks to vertex ids of `topo`: the nearest vertex in the domain that lies
/// on the requested side of a cut.
pub(crate) fn resolve(topo: &Topology) -> Vec<u32> {
    targets(topo)
        .into_iter()
        .map(|(dom, on)| {
            let ok = |eye: Side, mouth: Side| match on {
                On::Skin => eye != Side::Lower && mouth != Side::Lower,
                On::EyeUpper => eye == Side::Upper,
                On::EyeLower => eye == Side::Lower,
                On::MouthUpper => mouth == Side::Upper,
                On::MouthLower => mouth == Side::Lower,
            };
            topo.vertices
                .iter()
                .enumerate()
                .filter(|(_, v)| ok(v.eye, v.mouth))
                .min_by(|(_, a), (_, b)| {
                    let da = (a.dom[0] - dom[0]).powi(2) + (a.dom[1] - dom[1]).powi(2);
                    let db = (b.dom[0] - dom[0]).powi(2) + (b.dom[1] - dom[1]).powi(2);
                    da.total_cmp(&db)
                })
                .map(|(i, _)| i as u32)
                .expect("a vertex on every side")
        })
        .collect()
}

/// Delaunay triangulation of 2D points (Bowyer–Watson), as index triples, counter-clockwise
/// for `+y` up. Points must be distinct.
///
/// Over the landmarks this gives the classic face triangulation used for piecewise-affine
/// warps (Active Appearance Models, face morphing): each triangle maps image texture between
/// two landmark shapes.
pub fn delaunay(points: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let p: Vec<[f64; 2]> = points
        .iter()
        .map(|q| [f64::from(q[0]), f64::from(q[1])])
        .collect();
    let n = p.len();
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for q in &p {
        for k in 0..2 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let r = 20.0 * (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-9);
    // Super-triangle vertices get indices n, n+1, n+2.
    let mut pts = p.clone();
    pts.extend([[c[0] - r, c[1] - r], [c[0] + r, c[1] - r], [c[0], c[1] + r]]);

    let ccw = |t: [usize; 3]| {
        let [a, b, d] = t.map(|i| pts[i]);
        (b[0] - a[0]) * (d[1] - a[1]) - (b[1] - a[1]) * (d[0] - a[0]) > 0.0
    };
    let in_circle = |t: [usize; 3], q: [f64; 2]| {
        let [a, b, d] = t.map(|i| [pts[i][0] - q[0], pts[i][1] - q[1]]);
        let det = (a[0] * a[0] + a[1] * a[1]) * (b[0] * d[1] - d[0] * b[1])
            - (b[0] * b[0] + b[1] * b[1]) * (a[0] * d[1] - d[0] * a[1])
            + (d[0] * d[0] + d[1] * d[1]) * (a[0] * b[1] - b[0] * a[1]);
        det > 0.0 // triangles are kept counter-clockwise
    };

    let mut tris = vec![[n, n + 1, n + 2]];
    for (i, &q) in p.iter().enumerate() {
        let (bad, good): (Vec<_>, Vec<_>) = tris.into_iter().partition(|&t| in_circle(t, q));
        tris = good;
        // The cavity's boundary: edges of bad triangles not shared with another bad one.
        let edges: Vec<[usize; 2]> = bad
            .iter()
            .flat_map(|t| [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]])
            .collect();
        for e in &edges {
            if !edges.iter().any(|f| f == &[e[1], e[0]]) {
                let t = [e[0], e[1], i];
                tris.push(if ccw(t) { t } else { [e[1], e[0], i] });
            }
        }
    }
    tris.retain(|t| t.iter().all(|&i| i < n));
    tris
}

/// iBUG mirror partner of each landmark (`MIRROR[i]` is `i` reflected across the face).
pub const MIRROR: [usize; LANDMARK_COUNT] = [
    16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, // jaw
    26, 25, 24, 23, 22, 21, 20, 19, 18, 17, // brows
    27, 28, 29, 30, 35, 34, 33, 32, 31, // nose
    45, 44, 43, 42, 47, 46, 39, 38, 37, 36, 41, 40, // eyes
    54, 53, 52, 51, 50, 49, 48, 59, 58, 57, 56, 55, // outer lips
    64, 63, 62, 61, 60, 67, 66, 65, // inner lips
];
