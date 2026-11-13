//! Drawing a [`FaceRig`] with game-toolkit (feature `toolkit`).

use game_toolkit::prelude::{Color, Graphics, Mat4, MeshId, MeshVertex, Painter, transform};

use crate::math::{Vec3, Xf, cross, normalize, sub};
use crate::regions::{self, Region};
use crate::rig::FaceRig;
use crate::topology::Detail;

/// Colors of the face's parts.
#[derive(Copy, Clone, Debug)]
pub struct FaceColors {
    pub skin: Color,
    pub sclera: Color,
    pub iris: Color,
    pub pupil: Color,
    pub mouth: Color,
    pub brows: Color,
    pub lips: Color,
}

impl Default for FaceColors {
    fn default() -> Self {
        Self {
            skin: [0.86, 0.68, 0.58, 1.0].into(),
            sclera: [0.95, 0.94, 0.92, 1.0].into(),
            iris: [0.25, 0.42, 0.55, 1.0].into(),
            pupil: [0.03, 0.03, 0.04, 1.0].into(),
            mouth: [0.25, 0.06, 0.07, 1.0].into(),
            brows: [0.24, 0.16, 0.11, 1.0].into(),
            lips: [0.74, 0.44, 0.43, 1.0].into(),
        }
    }
}

/// GPU meshes for a [`FaceRig`]: the deforming skin plus a shared sphere for the eyeballs,
/// irises, pupils and mouth cavity.
pub struct FaceModel {
    skin: MeshId,
    sphere: MeshId,
    vertices: Vec<MeshVertex>,
    brows: Decal,
    lips: Decal,
    pub colors: FaceColors,
}

/// A skin region drawn in its own color.
struct Decal {
    mesh: MeshId,
    decal: regions::Decal,
    vertices: Vec<MeshVertex>,
}

/// How far decals float above the skin (head units), enough to win the depth test.
const DECAL_LIFT: f32 = 0.006;

impl Decal {
    fn new(gfx: &mut Graphics, rig: &FaceRig, region: Region) -> Self {
        let decal = regions::Decal::new(rig.topology(), region);
        let vertices = Self::vertices(&decal, rig);
        Self {
            mesh: gfx.create_mesh(&vertices, &decal.indices),
            decal,
            vertices,
        }
    }

    fn vertices(decal: &regions::Decal, rig: &FaceRig) -> Vec<MeshVertex> {
        decal
            .evaluate(rig.positions(), rig.normals(), DECAL_LIFT)
            .into_iter()
            .map(|(p, n)| MeshVertex::new(p, n))
            .collect()
    }
}

impl FaceModel {
    /// Upload `rig`. A [`Detail::Low`] rig is flat-shaded (one normal per facet) for the
    /// low-poly look, eyes included.
    pub fn new(gfx: &mut Graphics, rig: &FaceRig) -> Self {
        let vertices = skin_vertices(rig);
        let skin = if flat(rig) {
            let indices: Vec<u16> = (0..vertices.len() as u16).collect();
            gfx.create_mesh(&vertices, &indices)
        } else {
            gfx.create_mesh(&vertices, rig.indices())
        };
        let (sv, si) = if flat(rig) {
            uv_sphere(6, 10)
        } else {
            uv_sphere(24, 32)
        };
        Self {
            skin,
            sphere: gfx.create_mesh(&sv, &si),
            vertices,
            brows: Decal::new(gfx, rig, Region::Brows),
            lips: Decal::new(gfx, rig, Region::Lips),
            colors: FaceColors::default(),
        }
    }

    /// Upload the rig's current positions and normals.
    pub fn sync(&mut self, gfx: &mut Graphics, rig: &FaceRig) {
        self.vertices = skin_vertices(rig);
        gfx.update_mesh(self.skin, &self.vertices);
        for d in [&mut self.brows, &mut self.lips] {
            d.vertices = Decal::vertices(&d.decal, rig);
            gfx.update_mesh(d.mesh, &d.vertices);
        }
    }

    /// Draw the face with `model` (head space to world) applied.
    pub fn draw(&self, p: &mut Painter<'_>, rig: &FaceRig, model: Mat4) {
        let c = self.colors;
        let at = |xf: &Xf, s: Vec3| transform::mul(&model, &xf.to_mat4(s));
        p.mesh(self.skin, model, c.skin);
        p.mesh(self.brows.mesh, model, c.brows);
        p.mesh(self.lips.mesh, model, c.lips);
        let r = rig.eye_radius();
        for eye in rig.eye_transforms() {
            p.mesh(self.sphere, at(&eye, [r, r, r]), c.sclera);
            // Iris and pupil: flattened spheres poking just through the front of the eyeball,
            // staying inside the closed lids.
            let iris = eye.compose(&Xf::translation([0.0, 0.0, r * 0.86]));
            p.mesh(
                self.sphere,
                at(&iris, [r * 0.38, r * 0.38, r * 0.16]),
                c.iris,
            );
            let pupil = eye.compose(&Xf::translation([0.0, 0.0, r * 0.9]));
            p.mesh(
                self.sphere,
                at(&pupil, [r * 0.16, r * 0.16, r * 0.135]),
                c.pupil,
            );
        }
        let (cavity, radii) = rig.mouth_cavity();
        p.mesh(self.sphere, at(&cavity, radii), c.mouth);
        // A static neck (it does not follow the head bone): an elongated sphere whose top
        // hides inside the head.
        let neck = Xf::translation([0.0, -1.35, -0.2]);
        p.mesh(self.sphere, at(&neck, [0.4, 1.0, 0.42]), c.skin);
    }
}

fn flat(rig: &FaceRig) -> bool {
    rig.topology().detail == Detail::Low
}

/// Skin vertices: shared and smooth-shaded, or for a flat-shaded rig three per triangle with
/// the facet's normal.
fn skin_vertices(rig: &FaceRig) -> Vec<MeshVertex> {
    let pos = rig.positions();
    if !flat(rig) {
        return pos
            .iter()
            .zip(rig.normals())
            .map(|(p, n)| MeshVertex::new(*p, *n))
            .collect();
    }
    rig.indices()
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|t| {
            let [a, b, c] = t.map(|i| pos[usize::from(i)]);
            let n = normalize(cross(sub(b, a), sub(c, a)));
            [a, b, c].map(|p| MeshVertex::new(p, n))
        })
        .collect()
}

/// Unit sphere with `rings` latitude bands and `segments` longitude columns.
fn uv_sphere(rings: u16, segments: u16) -> (Vec<MeshVertex>, Vec<u16>) {
    let mut v = Vec::new();
    for r in 0..=rings {
        let th = std::f32::consts::PI * f32::from(r) / f32::from(rings);
        for s in 0..=segments {
            let ph = std::f32::consts::TAU * f32::from(s) / f32::from(segments);
            let p = normalize([th.sin() * ph.sin(), th.cos(), th.sin() * ph.cos()]);
            v.push(MeshVertex::new(p, p));
        }
    }
    let mut i = Vec::new();
    let row = segments + 1;
    for r in 0..rings {
        for s in 0..segments {
            let (a, b) = (r * row + s, r * row + s + 1);
            let (c, d) = (a + row, b + row);
            i.extend([a, c, d, a, d, b]);
        }
    }
    (v, i)
}
