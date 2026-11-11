//! Headless smoke test of the editor: drives both pages with a scripted mouse against a recording fake host and checks that
//! it neither panics nor behaves non-deterministically, and that the expected parameter writes happen.
//! (nih-plug keeps its parameter setters crate-private, so values are not echoed back; the host write log is what is checked.)

use super::ui::State;
use crate::params::SatXCompParams;
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
    params: Arc<SatXCompParams>,
    shared: Arc<Shared>,
    st: State,
    t: f64,
    hash: u64,
}

impl Rig {
    fn new() -> Self {
        let ctx = egui::Context::default();
        satcomp_ui::init_ctx(&ctx);
        let params = Arc::new(SatXCompParams::default());
        Self { ctx, host: Arc::new(FakeHost::default()), shared: Arc::new(Shared::new()), st: State::new(&params), params, t: 1.0, hash: 0xcbf29ce484222325 }
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.t += 0.03;
        // some audio-side telemetry so meters and the spectrum draw
        let n = (self.t * 10.0) as f32;
        self.shared.opk[0].store(0.5 + 0.3 * n.sin(), std::sync::atomic::Ordering::Relaxed);
        self.shared.gr_pk[1].store(4.0 + n.cos().abs() * 3.0, std::sync::atomic::Ordering::Relaxed);
        for i in 0..8192usize {
            let v = 0.3 * ((i as f32 + n * 13.0) * 0.05).sin();
            self.shared.tap_out.buf[i].store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
            self.shared.tap_dry.buf[i].store((v * 1.4).to_bits(), std::sync::atomic::Ordering::Relaxed);
        }
        self.shared.tap_out.w.store(8192, std::sync::atomic::Ordering::Relaxed);
        self.shared.tap_dry.w.store(8192, std::sync::atomic::Ordering::Relaxed);
        let raw = RawInput { time: Some(self.t), screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1020.0, 720.0))), events, ..Default::default() };
        let setter = ParamSetter::new(&*self.host);
        let (p, sh) = (self.params.clone(), self.shared.clone());
        let st = &mut self.st;
        let full = self.ctx.run(raw, |ctx| super::run_frame(ctx, &setter, st, &p, &sh));
        for b in format!("{:?}", full.shapes).bytes() {
            self.hash ^= b as u64;
            self.hash = self.hash.wrapping_mul(0x100000001b3);
        }
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
    r.click(70.0, 42.0); // oversampling x2
    r.click(110.0, 42.0); // oversampling x4
    r.drag(41.0, 310.0, 41.0, 285.0); // DRIVE row of the selected band
    r.click(200.0, 268.0); // band row: select MID (click on the tab)
    r.drag(41.0, 310.0, 41.0, 295.0); // DRIVE of MID
    r.drag(100.0, 100.0, 150.0, 100.0); // viewport: grab a crossover line / marker area
    r.click(350.0, 268.0); // band row: HI tab
    r.click(450.0, 268.0); // DELTA of the band
    r.click(496.0, 268.0); // SOLO of the band
    r.click(175.0, 42.0); // EQ page
    r.drag(60.0, 120.0, 90.0, 80.0); // EQ node
    r.click(300.0, 268.0); // EQ band toggle
    r.drag(50.0, 352.0, 50.0, 327.0); // an EQ cell
    r.click(50.0, 291.0); // type glyph
    r.click(175.0, 42.0); // back to the main page
    r.click(630.0, 42.0); // BYPASS
    (r.hash, r.log())
}

#[test]
fn editor_runs_both_pages_and_writes_parameters() {
    let (h1, log) = script();
    let (h2, _) = script();
    assert_eq!(h1, h2, "editor output must be deterministic");
    let joined = log.join("\n");
    assert!(joined.contains("Sat Oversampling"), "oversampling click should reach the host:\n{joined}");
    assert!(joined.contains("Lo Drive"), "dragging DRIVE should write the selected band's drive:\n{joined}");
    assert!(joined.contains("Mid Drive"), "after selecting MID its drive must be the one written:\n{joined}");
    assert!(joined.contains("Hi Delta") && joined.contains("Hi Solo"), "band DELTA / SOLO act on the selected band:\n{joined}");
    assert!(joined.contains("EQ2 On") || joined.contains("EQ4 On"), "EQ band toggle should reach the host:\n{joined}");
    assert!(joined.contains("EQ1 Type"), "type glyph click should write the EQ type:\n{joined}");
    assert!(joined.contains("Bypass"), "BYPASS click should reach the host");
}

/// Regression: EQ-page cells whose parameter slot is 100 or above (EQ3-5 Q/freq/gain, tilt, pivot, mix, EQ gain) used to start
/// a drag that was never processed, and dragging an EQ node or a crossover line shared ids with those slots.
#[test]
fn eq_page_cells_above_slot_100_can_be_dragged() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    r.click(175.0, 42.0); // EQ page
    // (x, y, drag dy, parameter that must be written): cells start at y 282, 48 apart; EQ mix sits at its maximum
    for (x, y, dy, name) in [
        (470.0, 304.0, -25.0, "EQ Pivot"),
        (470.0, 352.0, -25.0, "EQ Tilt"),
        (470.0, 400.0, 25.0, "EQ Mix"),
        (470.0, 448.0, -25.0, "EQ Gain"),
        (210.0, 352.0, -25.0, "EQ3 Q"),
        (210.0, 400.0, -25.0, "EQ3 Freq"),
        (377.0, 448.0, -25.0, "EQ5 Gain"),
    ] {
        let n0 = r.log().len();
        r.drag(x, y, x, y + dy);
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name)), "dragging the {name} cell wrote nothing: {new:?}");
        assert!(new.iter().all(|l| l.starts_with(name)), "dragging the {name} cell also wrote other parameters: {new:?}");
    }
    // an EQ node drag must only move that node's own freq and gain (EQ2 sits at 200 Hz on the 0 dB line, x ~180, y 155)
    let n0 = r.log().len();
    r.drag(180.0, 155.0, 240.0, 120.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(new.iter().any(|l| l.starts_with("EQ2 Freq")) && new.iter().any(|l| l.starts_with("EQ2 Gain")), "{new:?}");
    assert!(new.iter().all(|l| l.starts_with("EQ2 ")), "an EQ node drag wrote other parameters: {new:?}");
}

/// The controls that moved or appeared in the 680 x 480 layout all reach the host.
#[test]
fn main_page_controls_of_the_wide_layout_write_parameters() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    // (x, y, drag dy, parameter): X-SLOPE rows are 22 high at y 210 / 232 (a slope step is ~55 px of drag), L.AHEAD is the small row
    // right of the band tabs, X-OVER rows sit above the slopes
    for (x, y, dy, name) in [
        (600.0, 221.0, -70.0, "X-Slope 1"),
        (600.0, 243.0, -70.0, "X-Slope 2"),
        (600.0, 268.0, -25.0, "Lookahead"),
        (600.0, 173.0, -25.0, "X-Over 1"),
        (600.0, 195.0, -25.0, "X-Over 2"),
    ] {
        let n0 = r.log().len();
        r.drag(x, y, x, y + dy);
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name)), "dragging {name} wrote nothing: {new:?}");
        assert!(new.iter().all(|l| l.starts_with(name)), "dragging {name} also wrote other parameters: {new:?}");
    }
    // band cells: THRESH of the selected (low) band, SAT GAIN, and the OUT column
    for (x, y, dy, name) in [
        (323.0, 304.0, -25.0, "Lo Thresh"),
        (207.0, 448.0, -25.0, "Lo Sat Gain"),
        (600.0, 448.0, -25.0, "Out Gain"),
        (600.0, 400.0, 25.0, "Mix"),
    ] {
        let n0 = r.log().len();
        r.drag(x, y, x, y + dy);
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name)), "dragging {name} wrote nothing: {new:?}");
        assert!(new.iter().all(|l| l.starts_with(name)), "dragging {name} also wrote other parameters: {new:?}");
    }
}
