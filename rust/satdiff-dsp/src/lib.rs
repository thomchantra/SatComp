//! DSP for SatDiff: the SatComp saturator (no compressor, no EQ) plus, in later steps, a phase rotator and a colour-fx slot
//! (Diffusor first). Built from the shared `satcomp-dsp` blocks. Design: docs/internal/SatDiff_spec.md.

pub mod diffuse;
pub mod engine;
pub mod params;
pub mod phase;

pub use engine::DiffEngine;
pub use params::DiffParams;
