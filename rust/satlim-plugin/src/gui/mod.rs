//! SatLim editor (the egui UI is the next layer; until then the host shows its generic parameter UI).

use crate::params::SatLimParams;
use crate::Shared;
use nih_plug::prelude::Editor;
use std::sync::Arc;

pub fn create(_params: Arc<SatLimParams>, _shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    None
}
