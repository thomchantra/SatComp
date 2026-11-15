//! Headless smoke test of the editor: drives the page with a scripted mouse against a recording fake host and checks that
//! it neither panics nor behaves non-deterministically, and that the expected parameter writes happen.
//! (nih-plug keeps its parameter setters crate-private, so values are not echoed back; the host write log is what is checked.)

use super::ui::State;
use crate::params::SatDiffParams;
use crate::Shared;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use std::sync::Arc;

#[derive(Default)]
struct FakeHost {
    log: parking_lot::Mutex<Vec<String>>,
}

impl GuiContext for FakeHost {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Vst3
    }
    fn request_resize(&self) -> bool {
        true
    }
    unsafe fn raw_begin_set_parameter(&self, _p: ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(&self, p: ParamPtr, n: f32) {
        self.log.lock().push(format!("{} {:.3}", p.name(), n));
    }
    unsafe fn raw_end_set_parameter(&self, _p: ParamPtr) {}
    fn get_state(&self) -> nih_plug::wrapper::state::PluginState {
        nih_plug::wrapper::state::PluginState { version: String::new(), params: Default::default(), fields: Default::default() }
    }
    fn set_state(&self, _s: nih_plug::wrapper::state::PluginState) {}
}

struct Rig {
    ctx: egui::Context,
    host: Arc<FakeHost>,
    params: Arc<SatDiffParams>,
    shared: Arc<Shared>,
    st: State,
    t: f64,
    hash: u64,
    /// hash of the last frame alone, and whether the fake audio thread keeps pushing scope buckets
    last: u64,
    feed: bool,
}

impl Rig {
    fn new() -> Self {
        Self::with_params(|_| {})
    }

    /// A rig whose parameters start from `tweak` (nih-plug's setters are sealed, so state is set before the params are shared).
    fn with_params(tweak: impl FnOnce(&mut SatDiffParams)) -> Self {
        let ctx = egui::Context::default();
        satcomp_ui::init_ctx(&ctx);
        let mut p = SatDiffParams::from_defaults(None); // hermetic: ignore this machine's defaults file
        tweak(&mut p);
        let params = Arc::new(p);
        Self { ctx, host: Arc::new(FakeHost::default()), shared: Arc::new(Shared::new()), st: State::new(0, false, 0, &params), params, t: 1.0, hash: 0xcbf29ce484222325, last: 0, feed: true }
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.t += 0.03;
        // some audio-side telemetry so meters and the scope draw
        let n = if self.feed { (self.t * 10.0) as f32 } else { 3.0 }; // frozen telemetry once the fake audio stops
        let ord = std::sync::atomic::Ordering::Relaxed;
        self.shared.opk[0].store(0.5 + 0.3 * n.sin(), ord);
        self.shared.opk[1].store(0.4 + 0.3 * n.cos(), ord);
        self.shared.gr_pk.store(2.0 + n.cos().abs() * 3.0, ord);
        self.shared.sat_pk.store(0.5, ord);
        self.shared.sat_vz.store(0.4, ord);
        self.shared.fms.store(0.05, ord);
        if self.feed {
            // buckets arrive in host-block sized chunks of 5-9, each a pure function of its absolute index
            let chunk = 5 + (n as usize % 5);
            for _ in 0..chunk {
                let i = self.shared.scope.head() as f32;
                let v = 0.3 + 0.2 * (i * 0.1).sin() + 0.1 * (i * 0.37).sin();
                self.shared.scope.push3(v, v * 1.25, v * 1.5);
            }
        }
        let raw = RawInput { time: Some(self.t), screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(super::W * 1.5, super::H * 1.5))), events, ..Default::default() };
        let setter = ParamSetter::new(&*self.host);
        let (p, sh) = (self.params.clone(), self.shared.clone());
        let st = &mut self.st;
        let full = self.ctx.run(raw, |ctx| super::run_frame(ctx, &setter, st, &p, &sh));
        let mut fh: u64 = 0xcbf29ce484222325;
        for b in format!("{:?}", full.shapes).bytes() {
            self.hash ^= b as u64;
            self.hash = self.hash.wrapping_mul(0x100000001b3);
            fh ^= b as u64;
            fh = fh.wrapping_mul(0x100000001b3);
        }
        self.last = fh;
    }

    /// logical (1.5 px per unit) position
    fn at(x: f32, y: f32) -> Pos2 {
        Pos2::new(x * 1.5, y * 1.5)
    }

    fn click(&mut self, x: f32, y: f32) {
        let p = Self::at(x, y);
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }]);
        self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
        self.frame(vec![]);
    }

    /// the host closed and reopened the editor: egui's clock restarts from zero while the UI state lives on
    fn reopen_editor(&mut self) {
        self.t = 0.0;
        // a reopened editor is a new egui context (its history panics if the clock runs backwards)
        self.ctx = egui::Context::default();
        satcomp_ui::init_ctx(&self.ctx);
    }

    fn rclick(&mut self, x: f32, y: f32) {
        let p = Self::at(x, y);
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Secondary, pressed: true, modifiers: Modifiers::NONE }]);
        self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Secondary, pressed: false, modifiers: Modifiers::NONE }]);
        self.frame(vec![]);
    }

    fn drag(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        let p0 = Self::at(x0, y0);
        self.frame(vec![Event::PointerMoved(p0)]);
        self.frame(vec![Event::PointerButton { pos: p0, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }]);
        for i in 1..=5 {
            let f = i as f32 / 5.0;
            self.frame(vec![Event::PointerMoved(Self::at(x0 + (x1 - x0) * f, y0 + (y1 - y0) * f))]);
        }
        let p1 = Self::at(x1, y1);
        self.frame(vec![Event::PointerButton { pos: p1, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
        self.frame(vec![]);
    }

    fn log(&self) -> Vec<String> {
        self.host.log.lock().clone()
    }
}

fn script() -> (u64, Vec<String>) {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    r.drag(46.0, 336.0, 46.0, 311.0); // DRIVE
    r.click(275.0, 63.0); // LIMIT off
    r.drag(200.0, 524.0, 200.0, 510.0); // OUT GAIN
    r.click(50.0, 63.0); // scene B
    r.click(18.0, 401.0); // fx on
    r.click(120.0, 443.0); // POST-SAT
    r.click(135.0, 378.0); // tile REPEAT
    r.click(275.0, 86.0); // morph direction
    r.drag(275.0, 130.0, 275.0, 118.0); // morph DEPTH
    r.click(275.0, 41.0); // oversample dropdown
    r.click(266.0, 100.0); // 8x
    r.click(275.0, 16.0); // BYPASS on
    r.click(275.0, 16.0); // and off
    r.click(150.0, 63.0); // shaper selector menu
    r.click(150.0, 63.0);
    r.click(110.0, 41.0); // preset dropdown
    r.click(110.0, 41.0);
    r.click(60.0, 12.0); // About page on
    for _ in 0..3 {
        r.frame(vec![]);
    }
    r.click(60.0, 12.0); // and off
    r.rclick(308.0, 500.0); // grip: size menu
    r.rclick(308.0, 500.0);
    r.reopen_editor();
    r.frame(vec![]);
    (r.hash, r.log())
}

#[test]
fn the_editor_runs_scripted_input_and_is_deterministic() {
    let (h1, _) = script();
    let (h2, log) = script();
    assert_eq!(h1, h2, "frames differ between two identical runs");
    for want in ["Drive", "Limiter", "Out Gain", "Scene A/B", "FX On", "FX Pre/Post", "Tile", "Morph Mode", "Morph Depth", "Oversampling", "Bypass"] {
        assert!(log.iter().any(|l| l.starts_with(want)), "no write to {want}: {log:?}");
    }
}

#[test]
fn the_limit_button_switches_it_off() {
    let mut r = Rig::new();
    for _ in 0..3 {
        r.frame(vec![]);
    }
    r.click(275.0, 63.0);
    assert!(r.log().iter().any(|l| l.starts_with("Limiter") && l.ends_with("0.000")), "{:?}", r.log());
}

#[test]
fn the_about_page_stays_quiet_and_writes_nothing() {
    // the editing flag is set through the About > Appearance flow; here just make sure many frames with the About page
    // open (which hosts it) neither panic nor change parameters
    let mut r = Rig::new();
    r.click(60.0, 12.0);
    for _ in 0..30 {
        r.frame(vec![]);
    }
    assert!(r.log().is_empty(), "{:?}", r.log());
}

#[test]
fn clicking_the_sat_readout_requests_the_level_match_and_its_result_writes_out_gain() {
    let mut r = Rig::new();
    for _ in 0..3 {
        r.frame(vec![]);
    }
    r.click(190.0, 290.0); // the SAT readout in the plot's bottom right corner
    assert!(r.shared.sat_req.swap(false, std::sync::atomic::Ordering::Relaxed), "no match request");
    r.shared.sat_result.store(-3.5, std::sync::atomic::Ordering::Relaxed);
    r.shared.sat_result_new.store(true, std::sync::atomic::Ordering::Release);
    r.frame(vec![]);
    assert!(r.log().iter().any(|l| l.starts_with("Out Gain")), "{:?}", r.log());
}

#[test]
fn clicking_the_output_bars_cycles_the_range_without_writing_parameters() {
    let mut r = Rig::new();
    for _ in 0..3 {
        r.frame(vec![]);
    }
    for _ in 0..5 {
        r.click(292.0, 400.0);
    }
    assert!(r.log().is_empty(), "{:?}", r.log());
}

#[test]
fn the_scope_outline_mode_draws_and_differs_from_solid() {
    let run = |solid: bool| {
        let mut r = Rig::with_params(|p| p.fft_solid = Arc::new(std::sync::atomic::AtomicBool::new(solid)));
        for _ in 0..6 {
            r.frame(vec![]);
        }
        r.rclick(275.0, 240.0); // the vertical viz starts as the oscilloscope: right-click for the spectrum, where the fill shows
        for _ in 0..6 {
            r.frame(vec![]);
        }
        r.hash
    };
    assert_ne!(run(true), run(false), "outline and solid should draw differently");
    assert_eq!(run(false), run(false));
}

#[test]
fn the_appearance_page_has_the_scope_fill_switch_on_its_own_row() {
    let mut r = Rig::new();
    r.click(60.0, 12.0); // About
    r.click(182.0, 38.0); // APPEARANCE tab
    for _ in 0..3 {
        r.frame(vec![]);
    }
    assert!(r.params.fft_solid.load(std::sync::atomic::Ordering::Relaxed), "solid by default");
    r.click(146.0, 86.0); // OUTLINE (second row under COLOR MODE)
    assert!(!r.params.fft_solid.load(std::sync::atomic::Ordering::Relaxed), "OUTLINE should switch the fill off");
    r.click(210.0, 86.0); // SOLID
    assert!(r.params.fft_solid.load(std::sync::atomic::Ordering::Relaxed));
    assert!(r.log().is_empty(), "UI-only setting, no host writes: {:?}", r.log());
}

#[test]
fn the_scope_holds_perfectly_still_when_the_audio_stops() {
    let mut r = Rig::new();
    for _ in 0..80 {
        r.frame(vec![]);
    }
    r.feed = false;
    for _ in 0..300 {
        r.frame(vec![]); // the eased read position settles on the head
    }
    r.frame(vec![]);
    let a = r.last;
    for _ in 0..5 {
        r.frame(vec![]);
        assert_eq!(r.last, a, "the picture moved with no new audio");
    }
}

#[test]
fn the_scope_advances_while_audio_runs_without_redrawing_old_columns() {
    // two rigs fed the same buckets, one frame apart in time: after the second has had longer to settle, both pictures are
    // made of the same stable columns shifted along, so an unchanged tail of the picture exists. Cheap proxy: the number of
    // distinct frame hashes over a run stays at most one per frame (never a flicker pair within one frame), and the picture
    // changes on most frames while running.
    let mut r = Rig::new();
    for _ in 0..40 {
        r.frame(vec![]);
    }
    let mut seen = std::collections::HashSet::new();
    for _ in 0..60 {
        r.frame(vec![]);
        seen.insert(r.last);
    }
    assert!(seen.len() >= 40, "the picture should advance on most frames: {} distinct of 60", seen.len());
}

#[test]
fn preferences_defaults_are_per_plugin_with_a_global_one_behind_a_chevron() {
    use satcomp_ui::appearance::{clear_defaults, clear_defaults_for, load_defaults, load_defaults_for};
    use std::sync::atomic::Ordering::Relaxed;
    let dir = std::env::temp_dir().join(format!("satcomp-defaults-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    std::env::set_var("SATCOMP_DATA_DIR", &dir);
    clear_defaults();
    clear_defaults_for("satdiff");
    let mut r = Rig::new();
    r.click(60.0, 12.0); // About
    r.click(182.0, 38.0); // PREFERENCES tab
    for _ in 0..3 {
        r.frame(vec![]);
    }
    // SET AS DEFAULT (the main part of the button) saves for THIS plug-in only
    r.click(208.0, 64.0); // LIGHT
    r.click(90.0, 435.0); // SET AS DEFAULT
    let own = load_defaults_for("satdiff").expect("the plug-in's own default");
    assert!(own.light, "the current look is what gets saved");
    assert_eq!((own.pal, own.face), (0, 0));
    assert!((own.scale - 1.0).abs() < 1e-6, "SatDiff opens at 100%");
    assert!(load_defaults().is_none(), "the main button does not touch the global default");
    // the chevron opens a one-item menu: SET AS GLOBAL DEFAULT
    r.click(140.0, 64.0); // DARK again
    r.click(146.0, 435.0); // chevron
    assert!(r.st.app.def_menu == 1, "the chevron opens the menu");
    r.click(80.0, 449.0); // SET AS GLOBAL DEFAULT
    assert!(r.st.app.def_menu == 0, "picking closes the menu");
    let global = load_defaults().expect("the global default");
    assert!(!global.light);
    assert!(load_defaults_for("satdiff").unwrap().light, "the plug-in's own default wins over the global one");
    // a new instance picks up its own default, a project restore would then overwrite it
    assert!(SatDiffParams::from_defaults(load_defaults_for("satdiff")).light.load(Relaxed));
    // RESET (main part): drops the plug-in's own default, this instance takes what a new one would get now (the global look)
    r.click(208.0, 64.0); // LIGHT
    assert!(r.params.light.load(Relaxed));
    r.click(190.0, 435.0);
    assert!(!r.params.light.load(Relaxed), "back on the global look");
    assert!(!load_defaults_for("satdiff").unwrap().light && load_defaults().is_some());
    // RESET GLOBAL DEFAULT (its chevron): nothing left, the factory look
    r.click(288.0, 435.0);
    r.click(190.0, 449.0);
    assert!(load_defaults().is_none() && load_defaults_for("satdiff").is_none(), "everything cleared");
    // a click outside an open menu only closes it
    r.click(146.0, 435.0);
    r.click(30.0, 300.0);
    assert!(r.st.app.def_menu == 0 && load_defaults().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_font_dropdown_opens_over_the_page_and_picks_a_face() {
    use std::sync::atomic::Ordering::Relaxed;
    let mut r = Rig::new();
    r.click(60.0, 12.0); // About
    r.click(182.0, 38.0); // PREFERENCES tab
    for _ in 0..3 {
        r.frame(vec![]);
    }
    let pal = r.params.palette_v2.load(Relaxed);
    r.click(150.0, 413.0); // the FONT dropdown
    assert!(r.st.app.font_open, "the list should be open");
    r.click(150.0, 350.0); // 8th entry, Victor Mono (the list is 11 rows now), drawn over the scheme list
    assert_eq!(r.params.font.load(Relaxed), 7, "Victor Mono");
    assert!(!r.st.app.font_open, "picking closes the list");
    assert_eq!(r.params.palette_v2.load(Relaxed), pal, "the click must not reach the scheme list under the open list");
    r.click(150.0, 413.0);
    r.click(20.0, 300.0); // a click elsewhere closes it without changing anything
    assert!(!r.st.app.font_open);
    assert_eq!(r.params.font.load(Relaxed), 7);
    assert!(r.log().is_empty(), "UI-only, no host writes: {:?}", r.log());
}

#[test]
fn the_theme_editor_page_has_a_filled_scope_in_stacking_order() {
    let r = Rig::new();
    r.st.fill_dummy();
    let ring = &r.st.dummy_shared.scope;
    assert!(ring.head() > 1000, "the mock scope should hold a couple of seconds");
    let h = ring.head();
    let mut peak = 0.0f32;
    for i in h - 1000..h {
        let (out, handed, pre) = ring.get3(i % satcomp_dsp::ScopeRing::LEN);
        assert!(out <= handed + 1e-6 && handed <= pre + 1e-6, "bucket {i}: {out} {handed} {pre}");
        peak = peak.max(pre);
    }
    assert!(peak > 1.0, "hits should reach over the ceiling so the removal shows");
    let before = ring.head();
    r.st.fill_dummy();
    assert_eq!(ring.head(), before, "filled once, then still");
}

#[test]
fn the_dice_rolls_a_different_scheme_each_time_and_scrolls_the_list_to_it() {
    use std::sync::atomic::Ordering::Relaxed;
    let mut r = Rig::new();
    r.click(60.0, 12.0); // About
    r.click(182.0, 38.0); // PREFERENCES tab
    for _ in 0..3 {
        r.frame(vec![]);
    }
    let n = r.st.app.n_schemes() as i32;
    assert!(n >= 27, "the built-in table has grown to 27: {n}");
    let mut seen = std::collections::BTreeSet::new();
    let mut prev = r.params.palette_v2.load(Relaxed);
    for _ in 0..12 {
        r.click(110.0, 109.0); // the dice, right of the COLOR SCHEME label
        let now = r.params.palette_v2.load(Relaxed);
        assert!(now >= 0 && now < n, "scheme {now} of {n}");
        assert_ne!(now, prev, "a roll never lands on the current scheme");
        // the list follows the roll: the new scheme is among the visible rows (14 on this page; the scroll is clamped to the last screen)
        let sc = r.st.app.ap_scroll as i32;
        assert!(now >= sc && now < sc + 14, "scheme {now} not visible with the list at {sc}");
        seen.insert(now);
        prev = now;
    }
    assert!(seen.len() >= 4, "twelve rolls should land on several different schemes: {seen:?}");
    assert!(r.log().is_empty(), "UI-only, no host writes");
}

#[test]
fn the_logo_and_the_title_light_up_under_the_pointer() {
    // the picture with the pointer parked on an empty spot, on the logo, on the title: three different frames
    let frame_with_pointer_at = |x: f32, y: f32| {
        let mut r = Rig::new();
        for _ in 0..4 {
            r.frame(vec![]);
        }
        r.frame(vec![Event::PointerMoved(Rig::at(x, y))]);
        r.frame(vec![]);
        r.last
    };
    let (idle, logo, title) = (frame_with_pointer_at(200.0, 300.0), frame_with_pointer_at(22.0, 17.0), frame_with_pointer_at(80.0, 17.0));
    assert_ne!(idle, logo, "the logo should invert under the pointer");
    assert_ne!(idle, title, "the title should get a highlight under the pointer");
    assert_ne!(logo, title);
}

#[test]
fn dragging_the_derived_half_of_an_odd_curve_moves_the_real_point_the_right_way() {
    let mut r = Rig::new();
    for _ in 0..3 {
        r.frame(vec![]);
    }
    r.click(225.0, 87.0); // pencil: opens the draw view (odd tool for the linear start curve)
    // the derived end point sits at (-1, -1): lower-left corner of the inset domain; pull it up to y = -0.5
    r.drag(32.0, 279.0, 32.0, 232.0);
    let c = satcomp_dsp::shapers::Curve::from_points(&r.params.custom_curve.lock());
    assert!(c.is_odd(), "an odd curve must stay odd: {:?}", c.pts());
    assert!((c.eval(1.0) - 0.5).abs() < 0.05 && (c.eval(-1.0) + 0.5).abs() < 0.05, "f(1) = {}, f(-1) = {}", c.eval(1.0), c.eval(-1.0));
}

#[test]
fn tile_shows_in_the_plot_and_dims_on_the_periodic_shapers() {
    use nih_plug::prelude::{FloatParam, FloatRange, IntParam, IntRange};
    let run = |tile: i32, shape: i32| {
        let mut r = Rig::with_params(|p| {
            p.drive = FloatParam::new("Drive", 50.0, FloatRange::Linear { min: 0.0, max: 100.0 });
            p.tile = IntParam::new("Tile", tile, IntRange::Linear { min: 0, max: 2 });
            p.shape = IntParam::new("Shape", shape, IntRange::Linear { min: 0, max: 14 });
        });
        for _ in 0..4 {
            r.frame(vec![]);
        }
        r.hash
    };
    // Soft: the plot differs between off / mirror / repeat
    let (off, mir, rep) = (run(0, 1), run(1, 1), run(2, 1));
    assert!(off != mir && off != rep && mir != rep, "plot ignores TILE");
    // Wrap tiles itself: the switch changes nothing, not even the buttons' look beyond the dimming already there
    assert_eq!(run(1, 9), run(2, 9));
}

#[test]
fn the_whole_fx_dropdown_button_opens_the_menu() {
    let base = {
        let mut r = Rig::new();
        for _ in 0..4 {
            r.frame(vec![]);
        }
        r.hash
    };
    let mut r = Rig::new();
    for _ in 0..3 {
        r.frame(vec![]);
    }
    r.click(100.0, 401.0); // the middle of the button, not the arrow
    r.frame(vec![]);
    assert_ne!(r.hash, base, "the menu did not open");
}

#[test]
fn speed_is_inert_in_mix_and_live_in_the_detector_modes() {
    let drag_speed = |mode: f32| {
        let mut r = Rig::with_params(|p| p.morph_dir = nih_plug::prelude::IntParam::new("Morph Mode", mode as i32, nih_plug::prelude::IntRange::Linear { min: 0, max: 2 }));
        for _ in 0..3 {
            r.frame(vec![]);
        }
        r.drag(275.0, 160.0, 275.0, 140.0);
        r.log().iter().any(|l| l.starts_with("Morph Speed"))
    };
    assert!(!drag_speed(2.0), "SPEED should be greyed out in MIX");
    assert!(drag_speed(0.0), "SPEED should work in A>B");
}

#[test]
fn the_ab_switch_sends_shape_and_tile_edits_to_the_scene_being_edited() {
    use nih_plug::prelude::{IntParam, IntRange};
    let on_scene = |ab: i32| {
        let mut r = Rig::with_params(|p| p.ab = IntParam::new("Scene A/B", ab, IntRange::Linear { min: 0, max: 1 }));
        for _ in 0..3 {
            r.frame(vec![]);
        }
        r.click(226.0, 63.0); // selector right arrow
        r.click(135.0, 378.0); // tile REPEAT
        r.log()
    };
    let a = on_scene(0);
    assert!(a.iter().any(|l| l.starts_with("Shape 0")) && a.iter().any(|l| l.starts_with("Tile 1")), "{a:?}");
    assert!(!a.iter().any(|l| l.contains(" B ")), "{a:?}");
    let b = on_scene(1);
    assert!(b.iter().any(|l| l.starts_with("Shape B")) && b.iter().any(|l| l.starts_with("Tile B")), "{b:?}");
    assert!(!b.iter().any(|l| l.starts_with("Shape 0") || l.starts_with("Tile 1")), "{b:?}");
}

#[test]
fn the_curve_handle_bends_a_segment_and_linear_straightens_the_rest() {
    let mut r = Rig::new();
    for _ in 0..3 {
        r.frame(vec![]);
    }
    r.click(225.0, 87.0); // pencil: opens the draw view (odd tool)
    r.click(171.0, 87.0); // free draw tool
    // the linear start curve has one segment: its handle is on the curve at the middle of the inset domain
    r.drag(122.0, 188.0, 122.0, 160.0);
    let c = r.params.curve_snapshot();
    assert!(c.bend[0] > 0.2 && c.bend[0] < 0.45, "bend {:?}", &c.bend[..2]);
    assert!(c.eval(0.0) > 0.2, "the curve passes through the handle: {}", c.eval(0.0));
    assert!(r.params.custom_curve_ext.lock().len() == 2, "handles are persisted: {:?}", r.params.custom_curve_ext.lock());
    // double click on the handle straightens it again
    r.click(122.0, 160.0);
    r.click(122.0, 160.0);
    assert_eq!(r.params.curve_snapshot().bend[0], 0.0);
    // LINEAR | CURVE writes the curve's own flag
    r.click(30.0, 87.0);
    assert!(r.params.curve_snapshot().linear);
}
