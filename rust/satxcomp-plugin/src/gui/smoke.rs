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
        Self::with_params(|_| {})
    }

    /// A rig whose parameters start from `tweak` (nih-plug's setters are sealed, so state is set before the params are shared).
    fn with_params(tweak: impl FnOnce(&mut SatXCompParams)) -> Self {
        let ctx = egui::Context::default();
        satcomp_ui::init_ctx(&ctx);
        let mut p = SatXCompParams::default();
        tweak(&mut p);
        let params = Arc::new(p);
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
        let raw = RawInput { time: Some(self.t), screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1020.0, 615.0))), events, ..Default::default() };
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
    r.click(610.0, 176.0); // oversampling x2 (right column, bottom row)
    r.click(650.0, 176.0); // oversampling x4
    r.drag(41.0, 245.0, 41.0, 220.0); // DRIVE row of the selected band
    r.click(200.0, 198.0); // band row: select MID (click on the tab)
    r.drag(41.0, 245.0, 41.0, 230.0); // DRIVE of MID
    r.drag(100.0, 100.0, 150.0, 100.0); // viewport: grab a crossover line / marker area
    r.click(350.0, 198.0); // band row: HI tab
    r.click(468.0, 198.0); // DELTA of the band
    r.click(516.0, 198.0); // SOLO of the band
    r.click(530.0, 16.0); // EQ page (header)
    r.drag(60.0, 120.0, 90.0, 80.0); // EQ node
    r.click(300.0, 198.0); // EQ band toggle
    r.drag(50.0, 282.0, 50.0, 257.0); // an EQ cell
    r.click(50.0, 223.0); // type glyph
    r.click(530.0, 16.0); // back to the main page
    r.click(640.0, 42.0); // BYPASS
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
    r.click(530.0, 16.0); // EQ page
    // (x, y, drag dy, parameter that must be written): cells start at y 212, 48 apart; EQ mix sits at its maximum
    for (x, y, dy, name) in [
        (490.0, 234.0, -25.0, "EQ Pivot"),
        (490.0, 282.0, -25.0, "EQ Tilt"),
        (490.0, 330.0, 25.0, "EQ Mix"),
        (490.0, 378.0, -25.0, "EQ Gain"),
        (222.0, 282.0, -25.0, "EQ3 Q"),
        (222.0, 330.0, -25.0, "EQ3 Freq"),
        (394.0, 378.0, -25.0, "EQ5 Gain"),
    ] {
        let n0 = r.log().len();
        r.drag(x, y, x, y + dy);
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name)), "dragging the {name} cell wrote nothing: {new:?}");
        assert!(new.iter().all(|l| l.starts_with(name)), "dragging the {name} cell also wrote other parameters: {new:?}");
    }
    // an EQ node drag must only move that node's own freq and gain (EQ2 sits at 200 Hz on the 0 dB line, x ~187, y 109)
    let n0 = r.log().len();
    r.drag(187.0, 109.0, 247.0, 79.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(new.iter().any(|l| l.starts_with("EQ2 Freq")) && new.iter().any(|l| l.starts_with("EQ2 Gain")), "{new:?}");
    assert!(new.iter().all(|l| l.starts_with("EQ2 ")), "an EQ node drag wrote other parameters: {new:?}");
}

/// The controls of the 680 x 410 layout all reach the host.
#[test]
fn main_page_controls_of_the_wide_layout_write_parameters() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    // (x, y, drag dy, parameter): the crossover rows are 16 high at y 92 / 130, L.AHEAD is the small row at the end of the band row
    for (x, y, dy, name) in [(600.0, 100.0, -25.0, "X-Over 1"), (600.0, 138.0, -25.0, "X-Over 2"), (600.0, 198.0, -25.0, "Lookahead")] {
        let n0 = r.log().len();
        r.drag(x, y, x, y + dy);
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name)), "dragging {name} wrote nothing: {new:?}");
        assert!(new.iter().all(|l| l.starts_with(name)), "dragging {name} also wrote other parameters: {new:?}");
    }
    // slope buttons: 12 / 24 / 48 / 96, 30 wide from x 550, under each crossover
    for (x, y, name) in [(625.0, 116.0, "X-Slope 1"), (565.0, 154.0, "X-Slope 2")] {
        let n0 = r.log().len();
        r.click(x, y);
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name)) && new.iter().all(|l| l.starts_with(name)), "clicking a {name} button: {new:?}");
    }
    // oversampling lives under the crossover group
    let n0 = r.log().len();
    r.click(610.0, 176.0);
    assert!(r.log()[n0..].iter().any(|l| l.starts_with("Sat Oversampling")));
    // band cells: THRESH of the selected (low) band, SAT GAIN, and the OUT column
    for (x, y, dy, name) in [
        (343.0, 234.0, 25.0, "Lo Thresh"),
        (193.0, 378.0, -25.0, "Lo Sat Gain"),
        (610.0, 378.0, -25.0, "Out Gain"),
        (610.0, 330.0, 25.0, "Mix"),
    ] {
        let n0 = r.log().len();
        r.drag(x, y, x, y + dy);
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name)), "dragging {name} wrote nothing: {new:?}");
        assert!(new.iter().all(|l| l.starts_with(name)), "dragging {name} also wrote other parameters: {new:?}");
    }
}

/// Header: the preset bar is layout only (no parameter writes), SIDECHAIN toggles the key, the scale button opens the size menu.
#[test]
fn header_preset_layout_is_inert_and_scale_menu_opens() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    let n0 = r.log().len();
    for (x, y) in [(165.0, 16.0), (240.0, 16.0), (328.0, 16.0), (353.0, 16.0)] {
        r.click(x, y); // save icon, name, prev, next
    }
    assert!(r.log()[n0..].is_empty(), "the preset bar must not write parameters: {:?}", &r.log()[n0..]);
    r.click(460.0, 16.0); // SIDECHAIN
    assert!(r.log()[n0..].iter().any(|l| l.starts_with("Sidechain 1.000")), "SIDECHAIN click: {:?}", &r.log()[n0..]);
    let n0 = r.log().len();
    // the scale button opens a menu; clicking an entry must not touch parameters and a click elsewhere closes it
    r.click(395.0, 16.0);
    r.click(400.0, 24.0 + 1.0 + 14.0 * 2.0 + 7.0);
    r.click(395.0, 16.0);
    r.click(300.0, 300.0);
    assert!(r.log()[n0..].is_empty(), "scale menu must not write parameters: {:?}", &r.log()[n0..]);
}

/// EQ page: the dashed pivot line and empty space drive pivot and tilt, double-click resets a band.
#[test]
fn eq_viewport_pivot_tilt_and_band_reset() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    r.click(530.0, 16.0); // EQ page
    // the 600 Hz pivot line sits at x ~271 on the 528-wide log axis
    let n0 = r.log().len();
    r.drag(271.0, 60.0, 310.0, 60.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(!new.is_empty() && new.iter().all(|l| l.starts_with("EQ Pivot")), "pivot line drag: {new:?}");
    let n0 = r.log().len();
    r.drag(450.0, 60.0, 500.0, 60.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(!new.is_empty() && new.iter().all(|l| l.starts_with("EQ Tilt")), "sideways drag in empty space = tilt: {new:?}");
    // double-click on an EQ band toggle puts the whole band back (on, type, freq, Q, gain)
    r.click(300.0, 198.0);
    let n0 = r.log().len();
    r.click(300.0, 198.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    for p in ["EQ4 On", "EQ4 Type", "EQ4 Freq", "EQ4 Q", "EQ4 Gain"] {
        assert!(new.iter().any(|l| l.starts_with(p)), "double-click reset should write {p}: {new:?}");
    }
}

/// Saturator module: shaper dropdown (name click or right-click), drag in the plot = SAT GAIN, pencil + draw view per band.
#[test]
fn saturator_module_dropdown_plot_gain_and_curve_drawing() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    // the selector is y 212..234 (x 10..164), the plot y 260..400; the 15-entry menu is clamped to the 410-high canvas, so its
    // top is at y 198 and each entry is 14 high
    let n0 = r.log().len();
    r.rclick(80.0, 223.0);
    r.click(30.0, 198.0 + 1.0 + 3.0 * 14.0 + 7.0); // 4th entry
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(new.len() == 1 && new[0].starts_with("Lo Shape 0.214"), "right-click dropdown should pick shaper 3: {new:?}");
    let n0 = r.log().len();
    r.click(80.0, 223.0); // the name opens it as well
    r.click(30.0, 198.0 + 1.0 + 5.0 * 14.0 + 7.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(new.len() == 1 && new[0].starts_with("Lo Shape 0.357"), "name click dropdown should pick shaper 5: {new:?}");
    // a click outside closes the menu without writing, and is swallowed
    let n0 = r.log().len();
    r.click(80.0, 223.0);
    r.click(400.0, 100.0);
    assert!(r.log()[n0..].is_empty(), "a click outside the open menu must only close it: {:?}", &r.log()[n0..]);
    // dragging the plot adjusts the band's SAT GAIN
    let n0 = r.log().len();
    r.drag(80.0, 330.0, 80.0, 305.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(!new.is_empty() && new.iter().all(|l| l.starts_with("Lo Sat Gain")), "plot drag: {new:?}");
    // pencil: selects Custom and opens the draw view; a click adds a point, three clicks on it remove it again
    let n0 = r.log().len();
    r.click(142.0, 268.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(new.len() == 1 && new[0].starts_with("Lo Shape 1.000"), "pencil should select the Custom shaper: {new:?}");
    assert_eq!(r.params.curve_lo.lock().len(), 2);
    r.click(80.0, 330.0);
    assert_eq!(r.params.curve_lo.lock().len(), 3, "a click in the draw view adds a point");
    assert_eq!(r.params.curve_mid.lock().len(), 2, "other bands keep their own curve");
    r.click(80.0, 330.0);
    r.click(80.0, 330.0);
    assert_eq!(r.params.curve_lo.lock().len(), 2, "double-click removes the point");
    // the draw view edits the band that is selected
    r.click(200.0, 198.0); // MID tab
    r.click(142.0, 268.0);
    r.click(80.0, 330.0);
    assert_eq!(r.params.curve_mid.lock().len(), 3);
    assert_eq!(r.params.curve_lo.lock().len(), 2);
}

/// Band handles on the spectrum sit at the band's threshold: dragging one writes only that band's threshold.
#[test]
fn spectrum_handle_drag_sets_the_band_threshold() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    // default threshold 0 dB: the handle sits at the top of the 150-high viewport (y 34..184, centre clamped to y 41); the low
    // band's handle sits at its centre frequency (~55 Hz): x ~88
    let n0 = r.log().len();
    r.drag(88.0, 41.0, 88.0, 85.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(!new.is_empty() && new.iter().all(|l| l.starts_with("Lo Thresh")), "handle drag: {new:?}");
    // lower on screen = lower level: the last write must be below the default 0 dB (normalised 1.0)
    let last: f32 = new.last().unwrap().rsplit(' ').next().unwrap().parse().unwrap();
    assert!(last < 0.9, "dragging the handle down must lower the threshold: {new:?}");
    // double-click on the handle writes the default back (the rig does not echo values, so the handle stays at y 41)
    let n0 = r.log().len();
    r.click(88.0, 41.0);
    r.click(88.0, 41.0);
    assert!(r.log()[n0..].iter().any(|l| l.starts_with("Lo Thresh 1.000")), "double-click resets the threshold: {:?}", &r.log()[n0..]);
}

/// Sidechain on: the control row appears under the spectrum (SC gain, HP, LP writes; SOLO is a UI flag) and KEY MIX replaces
/// FF>FB in the compressor cells.
#[test]
fn sidechain_row_and_key_mix_cell() {
    let mut r = Rig::with_params(|p| p.sc_on = BoolParam::new("Sidechain", true));
    for _ in 0..4 {
        r.frame(vec![]);
    }
    // row y 167..184: SC gain x 10..170, HP 169..304, LP 303..438, SOLO 437..540
    for (x, y, dy, name) in [(90.0, 175.0, -25.0, "SC Gain"), (230.0, 175.0, -25.0, "SC HP"), (370.0, 175.0, 25.0, "SC LP")] {
        let n0 = r.log().len();
        r.drag(x, y, x, y + dy);
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name)) && new.iter().all(|l| l.starts_with(name)), "dragging {name}: {new:?}");
    }
    assert!(!r.shared.sc_solo.load(std::sync::atomic::Ordering::Relaxed));
    r.click(490.0, 175.0);
    assert!(r.shared.sc_solo.load(std::sync::atomic::Ordering::Relaxed), "SOLO click sets the monitor flag");
    // KEY MIX sits where FF>FB is (third compressor column, second row) and defaults to its maximum
    let n0 = r.log().len();
    r.drag(500.0, 282.0, 500.0, 307.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(new.iter().any(|l| l.starts_with("Key Mix")) && new.iter().all(|l| l.starts_with("Key Mix")), "KEY MIX cell: {new:?}");
    // sidechain off: solo is cleared by the next frame and the row is gone (no SC writes at its old spot)
    let mut r = Rig::new();
    r.shared.sc_solo.store(true, std::sync::atomic::Ordering::Relaxed);
    r.frame(vec![]);
    assert!(!r.shared.sc_solo.load(std::sync::atomic::Ordering::Relaxed), "solo never outlives the sidechain switch");
    r.drag(90.0, 175.0, 90.0, 150.0);
    assert!(!r.log().iter().any(|l| l.starts_with("SC Gain")), "no SC row while the sidechain is off: {:?}", r.log());
}
