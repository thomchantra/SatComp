mod ui;
#[cfg(test)]
mod golden;

use crate::params::SatCompParams;
use crate::Shared;
use satcomp_ui::canvas::{Canvas, FONT_NAMES, H, W};
use satcomp_ui::init_ctx;
use nih_plug::prelude::Editor;
use nih_plug_egui::egui::{self, LayerId};
use nih_plug_egui::create_egui_editor;
use std::sync::Arc;
use std::time::Duration;
use ui::{Env, Gui, State};

/// One editor frame: map the egui input into logical canvas units, draw the whole UI.
pub(crate) fn run_frame(
    ctx: &egui::Context,
    setter: &nih_plug::prelude::ParamSetter,
    st: &mut State,
    params: &SatCompParams,
    shared: &Shared,
) {
    let screen = ctx.screen_rect();
    let s = ((screen.width() / W).min(screen.height() / H) * 4.0).floor() / 4.0;
    let s = s.max(0.5);
    let ox = ((screen.width() - W * s) * 0.5).floor();
    let oy = ((screen.height() - H * s) * 0.5).floor();
    let ppp = ctx.pixels_per_point();
    let (pos, down, rdown, scroll, shift, now) = ctx.input(|i| {
        (
            i.pointer.latest_pos(),
            i.pointer.primary_down(),
            i.pointer.secondary_down(),
            i.raw_scroll_delta.y,
            i.modifiers.shift,
            i.time,
        )
    });
    let (mx, my) = match pos {
        Some(p) => ((p.x - ox) / s, (p.y - oy) / s),
        None => (-1000.0, -1000.0),
    };
    st.set_input(mx, my, down, rdown, scroll, shift, now);
    st.ptr_pt = pos.map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
    st.win_pt = (screen.width(), screen.height());

    let painter = ctx.layer_painter(LayerId::background());
    let cv = Canvas::new(&painter, ppp, ox, oy, s, st.colors(), st.face);
    painter.rect_filled(screen, 0.0, cv.bg_color());
    // Theme editor: the whole window is a static INIT page (default params, constant meter values); the real
    // params and shared state are not touched.
    let (dp, ds) = (st.dummy_params.clone(), st.dummy_shared.clone());
    let editing = st.app.editing();
    if editing {
        st.fill_dummy();
    }
    let (pr, sr): (&SatCompParams, &Shared) = if editing { (&*dp, &*ds) } else { (params, shared) };
    let env = Env { params: pr, setter, shared: sr, dsp: pr.to_dsp() };
    let mut g = Gui { s: st, cv, env };
    g.frame();
    ctx.request_repaint_after(Duration::from_millis(16));
    let _ = egui::Id::NULL;
}

pub fn create(params: Arc<SatCompParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    let (pal, light) = ui::initial_palette(&params);
    let face = params.font.load(std::sync::atomic::Ordering::Relaxed).clamp(0, FONT_NAMES.len() as i32 - 1) as usize;
    let state = State::new(pal, light, face, &params);
    let editor_state = params.editor_state.clone();
    create_egui_editor(
        editor_state,
        state,
        |ctx, _| init_ctx(ctx),
        move |ctx, setter, st| run_frame(ctx, setter, st, &params, &shared),
    )
}
