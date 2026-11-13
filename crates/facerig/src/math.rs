//! Minimal `[f32; 3]` vector math and rigid transforms for the rig.

pub type Vec3 = [f32; 3];

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: Vec3, s: f32) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn length(a: Vec3) -> f32 {
    dot(a, a).sqrt()
}

/// Unit vector along `a`, or zero for a (near) zero vector.
pub fn normalize(a: Vec3) -> Vec3 {
    let l = length(a);
    if l > 1e-12 {
        scale(a, 1.0 / l)
    } else {
        [0.0; 3]
    }
}

pub fn lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    add(a, scale(sub(b, a), t))
}

/// Hermite smoothstep; `e0 > e1` gives a falling edge.
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Unnormalised 2D Gaussian `exp(-(dx²/sx² + dy²/sy²))`.
pub fn gauss2(dx: f32, dy: f32, sx: f32, sy: f32) -> f32 {
    (-(dx * dx / (sx * sx) + dy * dy / (sy * sy))).exp()
}

/// A rigid transform: `p' = r · p + t`, with `r` a row-major rotation.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Xf {
    pub r: [[f32; 3]; 3],
    pub t: Vec3,
}

impl Xf {
    pub const IDENTITY: Xf = Xf {
        r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        t: [0.0; 3],
    };

    pub fn translation(t: Vec3) -> Self {
        Self {
            t,
            ..Self::IDENTITY
        }
    }

    /// Rotation about +x. Positive angles move the +z (front) point down.
    pub fn rot_x(a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Self {
            r: [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]],
            t: [0.0; 3],
        }
    }

    /// Rotation about +y. Positive angles turn the +z (front) point toward +x.
    pub fn rot_y(a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Self {
            r: [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]],
            t: [0.0; 3],
        }
    }

    /// Rotation about +z (roll).
    pub fn rot_z(a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Self {
            r: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
            t: [0.0; 3],
        }
    }

    /// `self · other` (apply `other` first, then `self`).
    pub fn compose(&self, other: &Xf) -> Xf {
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| self.r[i][k] * other.r[k][j]).sum();
            }
        }
        Xf {
            r,
            t: self.point(other.t),
        }
    }

    /// The same rotation about `pivot` instead of the origin.
    pub fn about(&self, pivot: Vec3) -> Xf {
        Xf::translation(pivot)
            .compose(self)
            .compose(&Xf::translation(scale(pivot, -1.0)))
    }

    /// The inverse transform (the rotation is orthonormal, so its inverse is its transpose).
    pub fn inverse(&self) -> Xf {
        let r = std::array::from_fn(|i| std::array::from_fn(|j| self.r[j][i]));
        let inv = Xf { r, t: [0.0; 3] };
        Xf {
            t: scale(inv.vector(self.t), -1.0),
            ..inv
        }
    }

    pub fn vector(&self, v: Vec3) -> Vec3 {
        [dot(self.r[0], v), dot(self.r[1], v), dot(self.r[2], v)]
    }

    pub fn point(&self, p: Vec3) -> Vec3 {
        add(self.vector(p), self.t)
    }

    /// Column-major 4×4 matrix with a non-uniform `scale` applied first (game-toolkit `Mat4`
    /// layout: `m[column][row]`).
    pub fn to_mat4(&self, scale: Vec3) -> [[f32; 4]; 4] {
        let mut m = [[0.0; 4]; 4];
        for (c, col) in m.iter_mut().take(3).enumerate() {
            for (r, v) in col.iter_mut().take(3).enumerate() {
                *v = self.r[r][c] * scale[c];
            }
        }
        m[3] = [self.t[0], self.t[1], self.t[2], 1.0];
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_follow_documented_signs() {
        let front = [0.0, 0.0, 1.0];
        assert!(Xf::rot_x(0.3).point(front)[1] < 0.0);
        assert!(Xf::rot_y(0.3).point(front)[0] > 0.0);
        let p = Xf::rot_x(0.7).about([0.0, 1.0, 0.0]).point([0.0, 1.0, 0.0]);
        assert!(length(sub(p, [0.0, 1.0, 0.0])) < 1e-6);
        let a = Xf::rot_y(0.4).compose(&Xf::translation([1.0, 0.0, 0.0]));
        let q = a.point([0.0; 3]);
        assert!((q[0] - 0.4f32.cos()).abs() < 1e-6 && (q[2] + 0.4f32.sin()).abs() < 1e-6);
        let b = Xf::rot_z(0.3).compose(&a).about([0.2, -0.1, 0.5]);
        let back = b.inverse().point(b.point([0.3, 0.7, -0.2]));
        assert!(length(sub(back, [0.3, 0.7, -0.2])) < 1e-6);
    }
}
