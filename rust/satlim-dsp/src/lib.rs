//! DSP for SatLim: the SatComp saturator module into an oversampled clipper and a true-peak brickwall limiter, built from
//! the shared `satcomp-dsp` blocks.

pub mod engine;
pub mod params;

pub use engine::LimEngine;
pub use params::LimParams;
