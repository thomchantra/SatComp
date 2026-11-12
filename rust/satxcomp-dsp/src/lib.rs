//! DSP for SatXComp, the 4-band multiband version of SatComp, built from the shared `satcomp-dsp` blocks.

pub mod confine;
pub mod engine;
pub mod params;
pub mod strip;
pub mod xover;

pub use engine::XEngine;
pub use xover::NB;
pub use params::{eq_kind, BandParams, EqBandParams, EqParams, XParams, EQ_DEFAULT_FREQ, EQ_DEFAULT_ON, EQ_DEFAULT_TYPE, EQ_KINDS};
