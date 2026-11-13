//! Bones and skinning: `head` (rotating about the neck) carries everything, `jaw` hangs off
//! the head and carries the lower face, and the eyeballs are rigid children of the head.
//! The skin uses linear blend skinning with a single procedural jaw weight per vertex.

use crate::math::{Vec3, Xf, add, cross, length, lerp, scale, smoothstep, sub};
use crate::params::{FaceParams, Param};
use crate::topology::{Side, Topology};

/// Jaw weight of every vertex: 1 on the chin and lower lip, 0 on the upper face, blending
/// across the cheeks beyond the mouth corners and fading out toward the ears and throat.
pub(crate) fn jaw_weights(topo: &Topology) -> Vec<f32> {
    let l = topo.layout;
    topo.vertices
        .iter()
        .map(|v| {
            let [x, y] = v.dom;
            let below = match v.mouth {
                Side::Lower => 1.0,
                Side::Upper => 0.0,
                Side::None => {
                    // Sharp at the mouth corners, softer further out on the cheeks.
                    let b = 0.015 + 0.5 * (x.abs() - l.mouth_half_w).max(0.0);
                    smoothstep(-b, b, l.mouth_y - y)
                }
            };
            let front = 1.0 - smoothstep(0.9, 1.5, x.abs());
            let throat = 1.0 - smoothstep(-1.2, -1.5, y);
            below * front * throat
        })
        .collect()
}

/// World transforms of the bones for a pose.
#[derive(Copy, Clone, Debug)]
pub struct Bones {
    pub head: Xf,
    /// Jaw motion in head space (identity when closed).
    pub jaw: Xf,
    /// Eye rotation in head space, about each eyeball centre.
    pub eye_rot: Xf,
}

pub(crate) fn pose(p: &FaceParams, jaw_pivot: Vec3, neck_pivot: Vec3) -> Bones {
    let head = Xf::rot_y(p.get(Param::HeadYaw))
        .compose(&Xf::rot_x(p.get(Param::HeadPitch)))
        .compose(&Xf::rot_z(p.get(Param::HeadRoll)))
        .about(neck_pivot);
    let jaw = Xf::translation([0.0, 0.0, p.get(Param::JawForward)]).compose(
        &Xf::rot_y(p.get(Param::JawSide))
            .compose(&Xf::rot_x(p.get(Param::JawOpen)))
            .about(jaw_pivot),
    );
    let eye_rot = Xf::rot_y(p.get(Param::EyeYaw)).compose(&Xf::rot_x(p.get(Param::EyePitch)));
    Bones { head, jaw, eye_rot }
}

/// Linear blend skinning of one point.
pub(crate) fn skin(b: &Bones, p: Vec3, jaw_w: f32) -> Vec3 {
    let local = if jaw_w > 0.0 {
        lerp(p, b.jaw.point(p), jaw_w)
    } else {
        p
    };
    b.head.point(local)
}

/// Area-weighted vertex normals of a triangle mesh.
pub(crate) fn normals(pos: &[Vec3], indices: &[u16]) -> Vec<Vec3> {
    let mut n = vec![[0.0f32; 3]; pos.len()];
    for t in indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(usize::from);
        let f = cross(sub(pos[b], pos[a]), sub(pos[c], pos[a]));
        for i in [a, b, c] {
            n[i] = add(n[i], f);
        }
    }
    n.into_iter()
        .map(|v| {
            let l = length(v);
            if l > 1e-12 {
                scale(v, 1.0 / l)
            } else {
                [0.0, 0.0, 1.0]
            }
        })
        .collect()
}
