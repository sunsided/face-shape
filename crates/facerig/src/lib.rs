//! A procedural 3D face rig: a hand-rolled head mesh deformed by three groups of parameters.
//!
//! - **Identity** (head proportions, nose, eyes, mouth, chin, …) sculpts the neutral face.
//! - **Expression**: FACS action units, built as Waters-style linear and sphincter muscles and
//!   baked into delta blend shapes, so expressions are linear in their weights.
//! - **Pose**: bones for the head, jaw and eyes, with linear blend skinning.
//!
//! All parameters live in one flat, bounded vector ([`FaceParams`], described by
//! [`PARAMS`]), and 68 iBUG-style [`landmarks`] are pinned to mesh vertices. That is the shape a
//! fitter needs to match the rig to a photo later on.
//!
//! ```
//! use game_toolkit_facerig::{FaceParams, FaceRig, Param};
//!
//! let mut rig = FaceRig::new();
//! rig.set_params(&FaceParams::default()
//!     .with(Param::LipCornerPuller, 0.8) // smile
//!     .with(Param::JawOpen, 0.1));
//! let nose_tip = rig.landmarks()[30];
//! assert!(nose_tip[2] > 0.0); // the face looks down +z
//! ```
//!
//! With the default `toolkit` feature, [`FaceModel`] uploads the rig to a game-toolkit mesh and
//! draws it.

#![forbid(unsafe_code)]

pub mod landmarks;
pub mod math;
mod muscles;
mod params;
pub mod regions;
mod rig;
mod shape;
mod skeleton;
#[cfg(feature = "toolkit")]
mod toolkit;
pub mod topology;

pub use params::{ACTION_UNITS, FaceParams, Group, PARAM_COUNT, PARAMS, Param, ParamSpec};
pub use rig::FaceRig;
#[cfg(feature = "toolkit")]
pub use toolkit::{FaceColors, FaceModel};
pub use topology::Detail;
