//! Kyber Cursor portable core.
//!
//! Platform-neutral rendering and settings code used by the Linux runtime in
//! `kyber-ignite`.
//!
//! Ported from `lib/build.py`, `lib/make_sounds.py` and `bin/kyber`
//! (color modes, random picker, volume curve, Xcursor writer).

pub mod color;
pub mod config;
pub mod geometry;
pub mod native;
pub mod overlay;
pub mod paths;
pub mod render;
pub mod sound_pack;
pub mod sounds;
pub mod theme;
pub mod trail;
pub mod xcursor;

pub use color::{darken, is_valid_hex, lighten, luminance};
pub use config::{ColorMode, KyberConfig, SoundMode};
pub use overlay::OverlayMeta;
