//! SatXComp editor: egui on the shared satcomp-ui canvas / widget kit, same logical-grid approach as SatComp.

mod ui;
#[cfg(test)]
mod smoke;

use crate::params::SatXCompParams;
use crate::Shared;
use nih_plug::prelude::Editor;
use nih_plug_egui::create_egui_editor;
use nih_plug_egui::egui::{self, LayerId};
use satcomp_ui::canvas::Canvas;
use satcomp_ui::init_ctx;
use std::sync::Arc;
use std::time::Duration;
use ui::{Env, Gui, State};

/// Logical grid (units); the window scales it in quarter steps.
pub const W: f32 = 680.0;
pub const H: f32 = 410.0;

/// One editor frame: map the egui input into logical canvas units, draw the whole UI.
pub(crate) fn run_frame(ctx: &egui::Context, setter: &nih_plug::prelude::ParamSetter, st: &mut State, params: &SatXCompParams, shared: &Shared) {
    let screen = ctx.screen_rect();
    let s = ((screen.width() / W).min(screen.height() / H) * 4.0).floor() / 4.0;
    let s = s.max(0.5);
    let ox = ((screen.width() - W * s) * 0.5).floor();
    let oy = ((screen.height() - H * s) * 0.5).floor();
    let ppp = ctx.pixels_per_point();
    let (pos, down, rdown, scroll, shift, now) = ctx.input(|i| {
        (i.pointer.latest_pos(), i.pointer.primary_down(), i.pointer.secondary_down(), i.raw_scroll_delta.y, i.modifiers.shift, i.time)
    });
    let (mx, my) = match pos {
        Some(p) => ((p.x - ox) / s, (p.y - oy) / s),
        None => (-1000.0, -1000.0),
    };
    st.set_input(mx, my, down, rdown, scroll, shift, now);
    let painter = ctx.layer_painter(LayerId::background());
    let cv = Canvas::new(&painter, ppp, ox, oy, s, st.colors(), st.face);
    painter.rect_filled(screen, 0.0, cv.bg_color());
    let env = Env { params, setter, shared, dsp: params.to_dsp() };
    let mut g = Gui { s: st, cv, env };
    g.frame();
    ctx.request_repaint_after(Duration::from_millis(16));
}

pub fn create(params: Arc<SatXCompParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    let state = State::new(&params);
    let editor_state = params.editor_state.clone();
    create_egui_editor(editor_state, state, |ctx, _| init_ctx(ctx), move |ctx, setter, st| run_frame(ctx, setter, st, &params, &shared))
}
