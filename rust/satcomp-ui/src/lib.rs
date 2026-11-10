//! Shared egui UI toolkit for the SatComp plug-in family.

pub mod canvas;
pub mod fft;
pub mod kit;

use canvas::font_family;
use nih_plug_egui::egui::{self, FontData, FontDefinitions};
use std::sync::Arc;

/// Editor init: the embedded UI fonts (see `canvas::FONT_NAMES`, same order) and the egui style tweaks the canvas relies on.
pub fn init_ctx(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let faces: [&'static [u8]; 6] = [
        include_bytes!("../assets/fonts/FiraCode-Regular.ttf"),
        include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
        include_bytes!("../assets/fonts/GeistMono-Regular.ttf"),
        include_bytes!("../assets/fonts/RobotoMono-Regular.ttf"),
        include_bytes!("../assets/fonts/OxygenMono-Regular.ttf"),
        include_bytes!("../assets/fonts/Doto-Black.ttf"),
    ];
    for (i, bytes) in faces.iter().enumerate() {
        fonts.font_data.insert(format!("f{i}"), Arc::new(FontData::from_static(bytes)));
        fonts.families.insert(font_family(i), vec![format!("f{i}")]);
    }
    ctx.set_fonts(fonts);
    ctx.tessellation_options_mut(|o| o.feathering = false);
    ctx.style_mut(|s| s.animation_time = 0.0);
}
