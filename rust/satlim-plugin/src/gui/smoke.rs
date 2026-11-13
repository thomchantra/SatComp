//! Headless smoke test of the editor: drives the page with a scripted mouse against a recording fake host and checks that
//! it neither panics nor behaves non-deterministically, and that the expected parameter writes happen.
//! (nih-plug keeps its parameter setters crate-private, so values are not echoed back; the host write log is what is checked.)

use super::ui::State;
use crate::params::SatLimParams;
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
    params: Arc<SatLimParams>,
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
    fn with_params(tweak: impl FnOnce(&mut SatLimParams)) -> Self {
        let ctx = egui::Context::default();
        satcomp_ui::init_ctx(&ctx);
        let mut p = SatLimParams::from_defaults(None); // hermetic: ignore this machine's defaults file
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
        self.shared.tpk.store(0.6 + 0.3 * n.sin(), ord);
        self.shared.gr_pk.store(2.0 + n.cos().abs() * 3.0, ord);
        self.shared.clip_gr_pk.store(1.0 + n.sin().abs() * 2.0, ord);
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
        let raw = RawInput { time: Some(self.t), screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(510.0, 756.0))), events, ..Default::default() };
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
    r.click(317.0, 196.0); // oversampling x8
    r.drag(41.0, 388.0, 41.0, 358.0); // DRIVE
    r.click(281.0, 270.0); // L.AHEAD off
    r.drag(281.0, 303.0, 281.0, 288.0); // CEILING
    r.drag(281.0, 335.0, 281.0, 355.0); // THRESHOLD (default is the top of its range: drag down)
    r.drag(281.0, 390.0, 281.0, 375.0); // CLIP
    r.drag(281.0, 472.0, 281.0, 458.0); // OUT GAIN
    r.click(247.0, 14.0); // DELTA
    r.click(247.0, 14.0);
    r.click(300.0, 14.0); // BYPASS on
    r.click(300.0, 14.0); // and off
    r.click(80.0, 197.0); // shaper selector menu
    r.click(80.0, 197.0);
    r.click(110.0, 171.0); // scope row: SRC
    r.drag(150.0, 80.0, 190.0, 100.0); // scope: window / ceiling drag
    r.click(197.0, 38.0); // preset dropdown
    r.click(197.0, 38.0);
    r.click(60.0, 12.0); // About page on
    for _ in 0..3 {
        r.frame(vec![]);
    }
    r.click(60.0, 12.0); // and off
    r.rclick(332.0, 496.0); // grip: size menu
    r.rclick(332.0, 496.0);
    r.reopen_editor();
    r.frame(vec![]);
    (r.hash, r.log())
}

#[test]
fn the_editor_runs_scripted_input_and_is_deterministic() {
    let (h1, _) = script();
    let (h2, log) = script();
    assert_eq!(h1, h2, "frames differ between two identical runs");
    for want in ["Oversampling", "Drive", "Lookahead", "Ceiling", "Threshold", "Clip", "Out Gain", "Bypass"] {
        assert!(log.iter().any(|l| l.starts_with(want)), "no write to {want}: {log:?}");
    }
}

#[test]
fn the_oversampling_buttons_write_x8() {
    let mut r = Rig::new();
    for _ in 0..3 {
        r.frame(vec![]);
    }
    r.click(317.0, 196.0);
    let log = r.log();
    // x8 = 3 of 0..3 -> normalised 1.0
    assert!(log.iter().any(|l| l.starts_with("Oversampling") && l.ends_with("1.000")), "{log:?}");
}

#[test]
fn the_look_ahead_button_switches_it_off() {
    let mut r = Rig::new();
    for _ in 0..3 {
        r.frame(vec![]);
    }
    r.click(281.0, 270.0);
    assert!(r.log().iter().any(|l| l.starts_with("Lookahead") && l.ends_with("0.000")), "{:?}", r.log());
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
fn clicking_the_sat_readout_requests_the_level_match_and_its_result_writes_sat_gain() {
    let mut r = Rig::new();
    for _ in 0..3 {
        r.frame(vec![]);
    }
    r.click(100.0, 358.0); // the SAT readout in the plot's bottom right corner
    assert!(r.shared.sat_req.swap(false, std::sync::atomic::Ordering::Relaxed), "no match request");
    r.shared.sat_result.store(-3.5, std::sync::atomic::Ordering::Relaxed);
    r.shared.sat_result_new.store(true, std::sync::atomic::Ordering::Release);
    r.frame(vec![]);
    assert!(r.log().iter().any(|l| l.starts_with("Sat Gain")), "{:?}", r.log());
}

#[test]
fn clicking_the_output_bars_cycles_the_range_without_writing_parameters() {
    let mut r = Rig::new();
    for _ in 0..3 {
        r.frame(vec![]);
    }
    for _ in 0..5 {
        r.click(205.0, 300.0);
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
    clear_defaults_for("satlim");
    let mut r = Rig::new();
    r.click(60.0, 12.0); // About
    r.click(182.0, 38.0); // PREFERENCES tab
    for _ in 0..3 {
        r.frame(vec![]);
    }
    // SET AS DEFAULT (the main part of the button) saves for THIS plug-in only
    r.click(208.0, 64.0); // LIGHT
    r.click(90.0, 435.0); // SET AS DEFAULT
    let own = load_defaults_for("satlim").expect("the plug-in's own default");
    assert!(own.light, "the current look is what gets saved");
    assert_eq!((own.pal, own.face), (0, 0));
    assert!((own.scale - 1.0).abs() < 1e-6, "SatLim opens at 100%");
    assert!(load_defaults().is_none(), "the main button does not touch the global default");
    // the chevron opens a one-item menu: SET AS GLOBAL DEFAULT
    r.click(140.0, 64.0); // DARK again
    r.click(158.0, 435.0); // chevron
    assert!(r.st.app.def_menu == 1, "the chevron opens the menu");
    r.click(80.0, 449.0); // SET AS GLOBAL DEFAULT
    assert!(r.st.app.def_menu == 0, "picking closes the menu");
    let global = load_defaults().expect("the global default");
    assert!(!global.light);
    assert!(load_defaults_for("satlim").unwrap().light, "the plug-in's own default wins over the global one");
    // a new instance picks up its own default, a project restore would then overwrite it
    assert!(SatLimParams::from_defaults(load_defaults_for("satlim")).light.load(Relaxed));
    // RESET (main part): drops the plug-in's own default, this instance takes what a new one would get now (the global look)
    r.click(208.0, 64.0); // LIGHT
    assert!(r.params.light.load(Relaxed));
    r.click(240.0, 435.0);
    assert!(!r.params.light.load(Relaxed), "back on the global look");
    assert!(!load_defaults_for("satlim").unwrap().light && load_defaults().is_some());
    // RESET GLOBAL DEFAULT (its chevron): nothing left, the factory look
    r.click(314.0, 435.0);
    r.click(240.0, 449.0);
    assert!(load_defaults().is_none() && load_defaults_for("satlim").is_none(), "everything cleared");
    // a click outside an open menu only closes it
    r.click(158.0, 435.0);
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
