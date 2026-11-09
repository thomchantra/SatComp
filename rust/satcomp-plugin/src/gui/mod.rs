mod canvas;
mod ui;

use crate::params::SatCompParams;
use crate::Shared;
use canvas::{fira, Canvas, H, W};
use nih_plug::prelude::Editor;
use nih_plug_egui::egui::{self, FontData, FontDefinitions, LayerId};
use nih_plug_egui::create_egui_editor;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use ui::{Env, Gui, State};

pub fn create(params: Arc<SatCompParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    let (pal, light) = ui::initial_palette(&params);
    let state = State::new(pal, light);
    let editor_state = params.editor_state.clone();
    create_egui_editor(
        editor_state,
        state,
        |ctx, _| {
            let mut fonts = FontDefinitions::default();
            fonts.font_data.insert(
                "fira".into(),
                Arc::new(FontData::from_static(include_bytes!("../../assets/FiraCode-Regular.ttf"))),
            );
            fonts.families.insert(fira(), vec!["fira".into()]);
            ctx.set_fonts(fonts);
            ctx.tessellation_options_mut(|o| o.feathering = false);
            ctx.style_mut(|s| s.animation_time = 0.0);
        },
        move |ctx, setter, st| {
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
            let cv = Canvas::new(&painter, ppp, ox, oy, s, st.pal, st.light);
            painter.rect_filled(screen, 0.0, cv.bg_color());
            let env = Env { params: &params, setter, shared: &shared, dsp: params.to_dsp() };
            let mut g = Gui { s: st, cv, env };
            g.frame();
            ctx.request_repaint_after(Duration::from_millis(16));
            let _ = egui::Id::NULL;
        },
    )
}
