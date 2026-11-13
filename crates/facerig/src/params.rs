//! The rig's parameters: one flat, bounded vector, described by the [`PARAMS`] table.
//!
//! Identity parameters are unitless offsets in `[-1, 1]` (0 = average face), action units are
//! activations in `[0, 1]`, and pose parameters are angles in radians or lengths in head units.
//! The flat layout is what a fitter optimises over later.

/// Which part of the rig a parameter drives.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    /// Who the face is: proportions and feature shapes. Changing these rebuilds the neutral.
    Identity,
    /// FACS action units: linear muscle blend shapes on top of the neutral.
    Expression,
    /// Bones: head, jaw and eyes.
    Pose,
}

/// Name, group and range of one parameter.
#[derive(Copy, Clone, Debug)]
pub struct ParamSpec {
    pub param: Param,
    /// Stable identifier (snake case), e.g. for saved files.
    pub name: &'static str,
    /// Human-readable slider label.
    pub label: &'static str,
    pub group: Group,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

macro_rules! params {
    ($($(#[$doc:meta])* $variant:ident = ($name:literal, $label:literal, $group:ident, $min:expr, $max:expr, $default:expr)),* $(,)?) => {
        /// Index of a rig parameter in [`FaceParams`].
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
        #[repr(usize)]
        pub enum Param {
            $($(#[$doc])* $variant),*
        }

        /// Every parameter, in [`Param`] order.
        pub const PARAMS: &[ParamSpec] = &[
            $(ParamSpec {
                param: Param::$variant,
                name: $name,
                label: $label,
                group: Group::$group,
                min: $min,
                max: $max,
                default: $default,
            }),*
        ];
    };
}

params! {
    // --- Identity ------------------------------------------------------------------------
    HeadWidth = ("head_width", "head width", Identity, -1.0, 1.0, 0.0),
    HeadHeight = ("head_height", "head height", Identity, -1.0, 1.0, 0.0),
    HeadDepth = ("head_depth", "head depth", Identity, -1.0, 1.0, 0.0),
    JawWidth = ("jaw_width", "jaw width", Identity, -1.0, 1.0, 0.0),
    ChinLength = ("chin_length", "chin length", Identity, -1.0, 1.0, 0.0),
    ChinProtrusion = ("chin_protrusion", "chin protrusion", Identity, -1.0, 1.0, 0.0),
    Cheekbones = ("cheekbones", "cheekbones", Identity, -1.0, 1.0, 0.0),
    BrowRidge = ("brow_ridge", "brow ridge", Identity, -1.0, 1.0, 0.0),
    NoseLength = ("nose_length", "nose length", Identity, -1.0, 1.0, 0.0),
    NoseWidth = ("nose_width", "nose width", Identity, -1.0, 1.0, 0.0),
    NoseProtrusion = ("nose_protrusion", "nose protrusion", Identity, -1.0, 1.0, 0.0),
    EyeSpacing = ("eye_spacing", "eye spacing", Identity, -1.0, 1.0, 0.0),
    EyeSize = ("eye_size", "eye size", Identity, -1.0, 1.0, 0.0),
    EyeAperture = ("eye_aperture", "eye aperture", Identity, -1.0, 1.0, 0.0),
    MouthWidth = ("mouth_width", "mouth width", Identity, -1.0, 1.0, 0.0),
    LipFullness = ("lip_fullness", "lip fullness", Identity, -1.0, 1.0, 0.0),
    // --- Expression (FACS action units) ---------------------------------------------------
    /// AU1, frontalis (medial).
    InnerBrowRaiser = ("au01", "AU1 inner brow raiser", Expression, 0.0, 1.0, 0.0),
    /// AU2, frontalis (lateral).
    OuterBrowRaiser = ("au02", "AU2 outer brow raiser", Expression, 0.0, 1.0, 0.0),
    /// AU4, corrugator supercilii and procerus.
    BrowLowerer = ("au04", "AU4 brow lowerer", Expression, 0.0, 1.0, 0.0),
    /// AU5, levator palpebrae superioris.
    UpperLidRaiser = ("au05", "AU5 upper lid raiser", Expression, 0.0, 1.0, 0.0),
    /// AU6, orbicularis oculi (orbital part).
    CheekRaiser = ("au06", "AU6 cheek raiser", Expression, 0.0, 1.0, 0.0),
    /// AU7, orbicularis oculi (palpebral part).
    LidTightener = ("au07", "AU7 lid tightener", Expression, 0.0, 1.0, 0.0),
    /// AU9, levator labii superioris alaeque nasi.
    NoseWrinkler = ("au09", "AU9 nose wrinkler", Expression, 0.0, 1.0, 0.0),
    /// AU10, levator labii superioris.
    UpperLipRaiser = ("au10", "AU10 upper lip raiser", Expression, 0.0, 1.0, 0.0),
    /// AU12, zygomaticus major.
    LipCornerPuller = ("au12", "AU12 lip corner puller", Expression, 0.0, 1.0, 0.0),
    /// AU15, depressor anguli oris.
    LipCornerDepressor = ("au15", "AU15 lip corner depressor", Expression, 0.0, 1.0, 0.0),
    /// AU17, mentalis.
    ChinRaiser = ("au17", "AU17 chin raiser", Expression, 0.0, 1.0, 0.0),
    /// AU18, orbicularis oris (pucker).
    LipPucker = ("au18", "AU18 lip pucker", Expression, 0.0, 1.0, 0.0),
    /// AU20, risorius.
    LipStretcher = ("au20", "AU20 lip stretcher", Expression, 0.0, 1.0, 0.0),
    /// AU23, orbicularis oris (tightening).
    LipTightener = ("au23", "AU23 lip tightener", Expression, 0.0, 1.0, 0.0),
    /// AU25, lips part.
    LipsPart = ("au25", "AU25 lips part", Expression, 0.0, 1.0, 0.0),
    /// AU43, eyes closed.
    EyesClosed = ("au43", "AU43 eyes closed", Expression, 0.0, 1.0, 0.0),
    // --- Pose (bones) ---------------------------------------------------------------------
    /// Head turn, radians; positive turns toward the face's left (+x).
    HeadYaw = ("head_yaw", "head yaw", Pose, -0.8, 0.8, 0.0),
    /// Head nod, radians; positive looks down.
    HeadPitch = ("head_pitch", "head pitch", Pose, -0.6, 0.6, 0.0),
    /// Head tilt, radians.
    HeadRoll = ("head_roll", "head roll", Pose, -0.5, 0.5, 0.0),
    /// Jaw opening, radians (AU26/27).
    JawOpen = ("jaw_open", "jaw open", Pose, 0.0, 0.35, 0.0),
    /// Jaw sideways rotation, radians (AU30).
    JawSide = ("jaw_side", "jaw sideways", Pose, -0.12, 0.12, 0.0),
    /// Jaw thrust, head units (AU29).
    JawForward = ("jaw_forward", "jaw forward", Pose, -0.04, 0.06, 0.0),
    /// Gaze left/right, radians; positive looks toward +x.
    EyeYaw = ("eye_yaw", "eye yaw", Pose, -0.5, 0.5, 0.0),
    /// Gaze up/down, radians; positive looks down.
    EyePitch = ("eye_pitch", "eye pitch", Pose, -0.4, 0.4, 0.0),
}

impl Param {
    pub fn spec(self) -> &'static ParamSpec {
        &PARAMS[self as usize]
    }
}

/// Number of parameters.
pub const PARAM_COUNT: usize = PARAMS.len();

/// The action units in [`Param`] order; their blend shapes are stored in this order too.
pub const ACTION_UNITS: &[Param] = &[
    Param::InnerBrowRaiser,
    Param::OuterBrowRaiser,
    Param::BrowLowerer,
    Param::UpperLidRaiser,
    Param::CheekRaiser,
    Param::LidTightener,
    Param::NoseWrinkler,
    Param::UpperLipRaiser,
    Param::LipCornerPuller,
    Param::LipCornerDepressor,
    Param::ChinRaiser,
    Param::LipPucker,
    Param::LipStretcher,
    Param::LipTightener,
    Param::LipsPart,
    Param::EyesClosed,
];

/// A full parameter vector, indexed by [`Param`]. Values are clamped to their spec range.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceParams {
    values: Vec<f32>,
}

impl Default for FaceParams {
    fn default() -> Self {
        Self {
            values: PARAMS.iter().map(|s| s.default).collect(),
        }
    }
}

impl FaceParams {
    pub fn get(&self, p: Param) -> f32 {
        self.values[p as usize]
    }

    /// Set `p`, clamped to its range.
    pub fn set(&mut self, p: Param, v: f32) {
        let s = p.spec();
        self.values[p as usize] = v.clamp(s.min, s.max);
    }

    /// Builder form of [`FaceParams::set`].
    pub fn with(mut self, p: Param, v: f32) -> Self {
        self.set(p, v);
        self
    }

    /// The raw vector in [`Param`] order.
    pub fn as_slice(&self) -> &[f32] {
        &self.values
    }

    /// Mutable raw access, e.g. for UI sliders. Call [`FaceParams::clamp`] after writing.
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.values
    }

    /// Clamp every value into its spec range.
    pub fn clamp(&mut self) {
        for (v, s) in self.values.iter_mut().zip(PARAMS) {
            *v = v.clamp(s.min, s.max);
        }
    }

    /// Reset every parameter of `group` to its default.
    pub fn reset_group(&mut self, group: Group) {
        for (v, s) in self.values.iter_mut().zip(PARAMS) {
            if s.group == group {
                *v = s.default;
            }
        }
    }

    /// Set every parameter of `group` uniformly at random within its range. `uniform` returns
    /// samples in `[0, 1)` (e.g. `game_toolkit::Rng::next_f32`); the crate has no RNG of its own.
    ///
    /// Head yaw, pitch and roll are left alone: they orient the whole head rather than shape
    /// the face.
    pub fn randomize_group(&mut self, group: Group, mut uniform: impl FnMut() -> f32) {
        for (v, s) in self.values.iter_mut().zip(PARAMS) {
            let head = matches!(s.param, Param::HeadYaw | Param::HeadPitch | Param::HeadRoll);
            if s.group == group && !head {
                *v = (s.min + uniform() * (s.max - s.min)).clamp(s.min, s.max);
            }
        }
    }

    /// The identity slice, used to detect when the neutral needs a rebuild.
    pub(crate) fn identity(&self) -> &[f32] {
        let n = PARAMS
            .iter()
            .take_while(|s| s.group == Group::Identity)
            .count();
        &self.values[..n]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_enum_and_groups_are_contiguous() {
        for (i, s) in PARAMS.iter().enumerate() {
            assert_eq!(s.param as usize, i, "{}", s.name);
            assert!(s.min < s.max && (s.min..=s.max).contains(&s.default));
        }
        let groups: Vec<_> = PARAMS.iter().map(|s| s.group).collect();
        let first_au = groups.iter().position(|g| *g == Group::Expression).unwrap();
        let first_pose = groups.iter().position(|g| *g == Group::Pose).unwrap();
        assert!(groups[..first_au].iter().all(|g| *g == Group::Identity));
        assert!(
            groups[first_au..first_pose]
                .iter()
                .all(|g| *g == Group::Expression)
        );
        assert!(groups[first_pose..].iter().all(|g| *g == Group::Pose));
        assert_eq!(ACTION_UNITS.len(), first_pose - first_au);
        for (k, au) in ACTION_UNITS.iter().enumerate() {
            assert_eq!(*au as usize, first_au + k);
        }
    }

    #[test]
    fn randomize_group_stays_in_range_and_leaves_other_groups() {
        let mut p = FaceParams::default();
        let mut x = 0.0f32;
        p.randomize_group(Group::Identity, || {
            x = (x + 0.37) % 1.0;
            x
        });
        for s in PARAMS {
            let v = p.get(s.param);
            assert!((s.min..=s.max).contains(&v), "{}", s.name);
            if s.group != Group::Identity {
                assert_eq!(v, s.default, "{}", s.name);
            }
        }
        assert_ne!(p, FaceParams::default());

        let mut p = FaceParams::default().with(Param::HeadYaw, 0.3);
        p.randomize_group(Group::Pose, || 0.9);
        assert_eq!(p.get(Param::HeadYaw), 0.3);
        assert_eq!(p.get(Param::HeadPitch), 0.0);
        assert_ne!(p.get(Param::JawOpen), 0.0);
    }

    #[test]
    fn set_clamps() {
        let mut p = FaceParams::default();
        p.set(Param::JawOpen, 9.0);
        assert_eq!(p.get(Param::JawOpen), Param::JawOpen.spec().max);
        p.as_mut_slice()[Param::HeadWidth as usize] = -5.0;
        p.clamp();
        assert_eq!(p.get(Param::HeadWidth), -1.0);
    }
}
