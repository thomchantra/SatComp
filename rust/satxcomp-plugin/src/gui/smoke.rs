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
        let mut p = SatXCompParams::from_defaults(None); // hermetic: ignore this machine's defaults file
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
        let raw = RawInput { time: Some(self.t), screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(996.0, 600.0))), events, ..Default::default() };
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

    /// the host closed and reopened the editor: egui's clock restarts from zero while the UI state lives on
    fn reopen_editor(&mut self) {
        self.t = 0.0;
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
    r.click(594.0, 184.0); // oversampling x2 (right column, bottom row)
    r.click(634.0, 184.0); // oversampling x4
    r.drag(41.0, 248.0, 41.0, 223.0); // DRIVE row of the selected band
    r.click(165.0, 206.0); // band row: select ML (click on the tab, four tabs of 104 px)
    r.drag(41.0, 248.0, 41.0, 233.0); // DRIVE of ML
    r.drag(100.0, 100.0, 150.0, 100.0); // viewport: grab a crossover line / marker area
    r.click(373.0, 206.0); // band row: HI tab
    r.click(452.0, 206.0); // DELTA of the band
    r.click(500.0, 206.0); // SOLO of the band
    r.click(514.0, 16.0); // EQ page (header)
    r.drag(60.0, 120.0, 90.0, 80.0); // EQ node
    r.click(300.0, 206.0); // EQ band toggle
    r.drag(50.0, 283.0, 50.0, 258.0); // an EQ cell
    r.click(50.0, 230.0); // type glyph
    r.click(514.0, 16.0); // back to the main page
    r.click(624.0, 42.0); // BYPASS
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
    assert!(joined.contains("ML Drive"), "after selecting ML its drive must be the one written:\n{joined}");
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
    r.click(514.0, 16.0); // EQ page
    // (x, y, drag dy, parameter that must be written): cells start at y 220, 44 apart; EQ mix sits at its maximum
    for (x, y, dy, name) in [
        (490.0, 239.0, -25.0, "EQ Pivot"),
        (490.0, 283.0, -25.0, "EQ Tilt"),
        (490.0, 327.0, 25.0, "EQ Mix"),
        (490.0, 371.0, -25.0, "EQ Gain"),
        (222.0, 283.0, -25.0, "EQ3 Q"),
        (222.0, 327.0, -25.0, "EQ3 Freq"),
        (394.0, 371.0, -25.0, "EQ5 Gain"),
    ] {
        let n0 = r.log().len();
        r.drag(x, y, x, y + dy);
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name)), "dragging the {name} cell wrote nothing: {new:?}");
        assert!(new.iter().all(|l| l.starts_with(name)), "dragging the {name} cell also wrote other parameters: {new:?}");
    }
    // an EQ node drag must only move that node's own freq and gain (EQ2 sits at 200 Hz on the 0 dB line, x ~182, y 113)
    let n0 = r.log().len();
    r.drag(182.0, 113.0, 242.0, 83.0);
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
    // (x, y, drag dy, parameter): the crossover rows are 16 high at y 110 / 132 / 154, L.AHEAD is the small row at the end of the band row
    for (x, y, dy, name) in [(560.0, 118.0, -25.0, "X-Over 1"), (560.0, 140.0, -25.0, "X-Over 2"), (560.0, 162.0, -25.0, "X-Over 3"), (584.0, 206.0, -25.0, "Lookahead")] {
        let n0 = r.log().len();
        r.drag(x, y, x, y + dy);
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name)), "dragging {name} wrote nothing: {new:?}");
        assert!(new.iter().all(|l| l.starts_with(name)), "dragging {name} also wrote other parameters: {new:?}");
    }
    // slope buttons at the right end of each crossover row: a click opens the 12 / 24 / 48 / 96 list, a pick writes the slope
    for (y, name) in [(118.0, "X-Slope 1"), (140.0, "X-Slope 2"), (162.0, "X-Slope 3")] {
        let n0 = r.log().len();
        r.click(637.0, y);
        assert!(r.log()[n0..].is_empty(), "opening the slope list must not write: {:?}", &r.log()[n0..]);
        r.click(595.0, y + 8.0 + 1.0 + 14.0 * 2.0 + 7.0); // third entry = 48 dB/oct
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name) && l.contains("0.667")) && new.iter().all(|l| l.starts_with(name)), "picking a {name} entry: {new:?}");
    }
    // oversampling lives under the crossover group
    let n0 = r.log().len();
    r.click(594.0, 184.0);
    assert!(r.log()[n0..].iter().any(|l| l.starts_with("Sat Oversampling")));
    // band cells: THRESH of the selected (low) band, SAT GAIN, and the OUT column
    for (x, y, dy, name) in [
        (327.0, 238.0, 25.0, "Lo Thresh"),
        (179.0, 371.0, -25.0, "Lo Sat Gain"),
        (594.0, 371.0, -25.0, "Out Gain"),
        (594.0, 327.0, 25.0, "Mix"),
    ] {
        let n0 = r.log().len();
        r.drag(x, y, x, y + dy);
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name)), "dragging {name} wrote nothing: {new:?}");
        assert!(new.iter().all(|l| l.starts_with(name)), "dragging {name} also wrote other parameters: {new:?}");
    }
}

/// Header: SIDECHAIN toggles the key, the scale button opens the size menu, the preset menu initialises (every stored
/// parameter, nothing excluded), the arrows step through the bank. The save icon is not clicked: it opens a real file dialog.
#[test]
fn header_sidechain_scale_menu_and_preset_menu() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    let n0 = r.log().len();
    r.click(444.0, 16.0); // SIDECHAIN
    assert!(r.log()[n0..].iter().any(|l| l.starts_with("Sidechain 1.000")), "SIDECHAIN click: {:?}", &r.log()[n0..]);
    // the scale button opens a menu; clicking an entry must not touch parameters and a click elsewhere closes it
    let n0 = r.log().len();
    r.click(379.0, 16.0);
    r.click(384.0, 24.0 + 1.0 + 14.0 * 2.0 + 7.0);
    r.click(379.0, 16.0);
    r.click(300.0, 300.0);
    assert!(r.log()[n0..].is_empty(), "scale menu must not write parameters: {:?}", &r.log()[n0..]);
    // name -> dropdown -> "Initialize preset" (first entry) writes every stored parameter and nothing excluded
    let n0 = r.log().len();
    r.click(240.0, 16.0);
    r.click(200.0, 24.0 + 1.0 + 7.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(new.len() >= 100, "initialise should write every stored parameter: {} writes", new.len());
    assert!(new.iter().any(|l| l.starts_with("Lo Drive 0.000")) && new.iter().any(|l| l.starts_with("ML Thresh 1.000")), "init values");
    for banned in ["Bypass", "Gain Match", "Sidechain"] {
        assert!(!new.iter().any(|l| l.starts_with(banned)), "{banned} must not be part of a preset");
    }
    // arrows step through factory + user presets (at least INIT itself): a full write each time
    for x in [312.0, 337.0] {
        let n0 = r.log().len();
        r.click(x, 16.0);
        assert!(r.log().len() - n0 >= 100, "stepping the bank applies a preset");
    }
}

/// The *NAME* marker: a loaded preset shows as modified once live parameters differ from it.
#[test]
fn preset_modified_marker_follows_the_parameters() {
    let mut clean = Rig::new();
    for _ in 0..4 {
        clean.frame(vec![]);
    }
    assert!(!clean.st.modified_marker(), "defaults equal INIT");
    let mut r = Rig::with_params(|p| p.lo_drive = FloatParam::new("Lo Drive", 50.0, FloatRange::Linear { min: 0.0, max: 100.0 }));
    for _ in 0..4 {
        r.frame(vec![]);
    }
    assert!(r.st.modified_marker(), "a changed parameter marks the preset as modified");
}

/// Sidechain on: the control row appears under the spectrum (SC gain, HP, LP writes; SOLO is a UI flag) and KEY MIX replaces
/// FF>FB in the compressor cells.
#[test]
fn sidechain_row_and_key_mix_cell() {
    let mut r = Rig::with_params(|p| p.sc_on = BoolParam::new("Sidechain", true));
    for _ in 0..4 {
        r.frame(vec![]);
    }
    // row y 175..192: SC gain x 10..170, HP 169..304, LP 303..438, SOLO 437..540
    for (x, y, dy, name) in [(90.0, 183.0, -25.0, "SC Gain"), (230.0, 183.0, -25.0, "SC HP"), (370.0, 183.0, 25.0, "SC LP")] {
        let n0 = r.log().len();
        r.drag(x, y, x, y + dy);
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name)) && new.iter().all(|l| l.starts_with(name)), "dragging {name}: {new:?}");
    }
    assert!(!r.shared.sc_solo.load(std::sync::atomic::Ordering::Relaxed));
    r.click(490.0, 183.0);
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
    r.drag(90.0, 183.0, 90.0, 158.0);
    assert!(!r.log().iter().any(|l| l.starts_with("SC Gain")), "no SC row while the sidechain is off: {:?}", r.log());
}

/// Reopening the editor restarts egui's clock; the slow GR number and the VU hold / clip timers must keep working afterwards
/// (they froze at 0 when their timestamps were not re-based).
#[test]
fn gr_number_and_meter_timers_survive_an_editor_reopen() {
    let mut r = Rig::new();
    for _ in 0..40 {
        r.shared.gr_pk[0].store(6.0, std::sync::atomic::Ordering::Relaxed);
        r.frame(vec![]);
    }
    assert!(r.st.gr_text(0) > 5.0, "GR number follows the engine before the reopen: {}", r.st.gr_text(0));
    r.reopen_editor();
    r.frame(vec![]);
    r.shared.gr_pk[0].store(0.0, std::sync::atomic::Ordering::Relaxed);
    for _ in 0..80 {
        r.frame(vec![]); // ~2.4 s on the restarted clock, no more GR
    }
    assert!(r.st.gr_text(0) < 0.5, "GR number must refresh after a reopen, not stay frozen: {}", r.st.gr_text(0));
}

/// About page: opens from the title, the Appearance tab switches scheme / mode and nothing reaches the host as a parameter.
#[test]
fn about_and_appearance_pages_work() {
    use std::sync::atomic::Ordering::Relaxed;
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    r.click(80.0, 15.0); // title -> About
    assert_eq!(r.st.app.about, 1);
    r.frame(vec![]);
    r.click(135.0, 42.0); // APPEARANCE tab
    assert_eq!(r.st.app.about, 3);
    r.click(80.0, 125.0); // second scheme row
    assert_eq!(r.params.palette_v2.load(Relaxed), 1);
    r.click(186.0, 68.0); // LIGHT
    assert!(r.params.light.load(Relaxed));
    r.click(135.0 + 115.0, 42.0); // CHANGELOG tab
    assert_eq!(r.st.app.about, 2);
    r.click(482.0, 42.0); // X CLOSE
    assert_eq!(r.st.app.about, 0);
    assert!(r.log().is_empty(), "the About pages must not write parameters: {:?}", r.log());
}

/// Theme editor: NEW THEME shows the dummy page with the editor cells, the picker opens on a swatch, and the real parameters are
/// never touched while it is open. (Saving would write the user's themes file, so the test cancels.)
#[test]
fn theme_editor_opens_edits_and_cancels_without_touching_parameters() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    r.click(80.0, 15.0);
    r.click(135.0, 42.0);
    r.click(380.0, 89.0); // NEW THEME
    assert!(r.st.app.editing());
    r.frame(vec![]);
    r.click(210.0 + 30.0, 220.0 + 20.0); // DARK BG swatch -> colour picker
    r.click(192.0, 104.0); // inside the saturation / value square
    r.drag(282.0, 74.0, 282.0, 114.0); // hue strip
    r.click(372.0, 161.0); // picker CANCEL
    r.click(480.0, 320.0); // editor cancel icon
    assert!(!r.st.app.editing());
    assert_eq!(r.st.app.about, 3, "cancel returns to the Appearance tab");
    assert!(r.log().is_empty(), "the editor must not write parameters: {:?}", r.log());
}

/// Icon: a double click toggles light / dark, a right click opens the scheme list.
#[test]
fn icon_double_click_toggles_light_and_right_click_lists_schemes() {
    use std::sync::atomic::Ordering::Relaxed;
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    r.click(20.0, 12.0);
    r.click(20.0, 12.0);
    assert!(r.params.light.load(Relaxed));
    for _ in 0..12 {
        r.frame(vec![]); // wait out the single-click window: the double click must not also cycle the scheme
    }
    assert_eq!(r.params.palette_v2.load(Relaxed), 0);
    r.rclick(20.0, 12.0);
    r.click(40.0, 12.0 + 22.0 + 2.0 + 14.0 * 3.0); // third schemes row: Furnace is index 1, rows: MODE, rule, Mono, Furnace
    assert_eq!(r.params.palette_v2.load(Relaxed), 1);
    r.click(20.0, 12.0); // single click: next scheme after the window
    for _ in 0..12 {
        r.frame(vec![]);
    }
    assert_eq!(r.params.palette_v2.load(Relaxed), 2);
}

/// ASYM heads the first stack of the saturator's right column (TONE under it): drag lowers / wheel changes the selected band's value for the current shaper (stored in the project).
#[test]
fn asym_slider_writes_the_selected_shapers_value() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    assert_eq!(r.params.asym_snapshot(0)[0], 100.0);
    r.drag(180.0, 228.0, 180.0, 248.0); // drag down = lower
    let a = r.params.asym_snapshot(0);
    assert!(a[0] < 100.0, "dragging ASYM changed nothing: {:?}", &a[..3]);
    assert_eq!(a[1], 100.0, "other shapers keep their own value");
    assert_eq!(r.params.asym_snapshot(1)[0], 100.0, "other bands keep their own value");
    r.click(180.0, 228.0);
    r.click(180.0, 228.0); // double-click = reset
    assert_eq!(r.params.asym_snapshot(0)[0], 100.0);
}

/// LIM sits left of the output meter and toggles the limiter parameter (on by default).
#[test]
fn lim_button_toggles_the_limiter() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    assert!(r.params.lim_on.value(), "limiter defaults to on");
    let n0 = r.log().len();
    r.click(549.0, 16.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(new.iter().any(|l| l.starts_with("Limiter 0.000")), "LIM click should switch the limiter off: {new:?}");
}

/// Waveform scope ("transient meter"): the icon in the viewport corner switches the view, the settings row appears under it,
/// dragging the view changes window / ceiling, the choice is saved with the project, and it writes no parameters.
#[test]
fn scope_icon_switches_the_view_and_drag_changes_window_and_ceiling() {
    use std::sync::atomic::Ordering::Relaxed;
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    assert_eq!(r.params.scope.load(Relaxed) & 1, 0);
    r.click(514.0, 43.0); // icon, top right of the viewport
    assert_eq!(r.params.scope.load(Relaxed) & 1, 1, "icon turns the scope on and stores it");
    let before = r.params.scope.load(Relaxed);
    r.drag(200.0, 80.0, 300.0, 110.0); // view drag: window and ceiling steps
    assert_ne!(r.params.scope.load(Relaxed), before, "dragging the view changes the settings");
    let n0 = r.log().len();
    r.click(40.0, 183.0); // SRC cell of the settings row (bottom of the viewport)
    assert_ne!((r.params.scope.load(Relaxed) >> 1) & 3, (before >> 1) & 3, "SRC cycles on click");
    assert_eq!(r.log().len(), n0, "the scope writes no parameters");
    r.click(514.0, 43.0);
    assert_eq!(r.params.scope.load(Relaxed) & 1, 0, "icon turns it off again");
}

/// The saturator's controls: BIAS beside DRIVE, ASYM over TONE and HP over LP in the right column, all writing the selected band.
#[test]
fn bias_tone_hp_lp_cells_write_the_selected_band() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    // (x, y, drag dy, parameter): selector 220-239, DRIVE | BIAS 239-258; stacks at x 142-216 from y 220 (halves 19 / 20 high)
    for (x, y, dy, name) in [
        (85.0, 248.0, -25.0, "Lo Bias"),
        (180.0, 250.0, 25.0, "Lo Tone"),
        (180.0, 266.0, -25.0, "Lo HP"),
        (180.0, 284.0, 25.0, "Lo LP"),
    ] {
        let n0 = r.log().len();
        r.drag(x, y, x, y + dy);
        let new: Vec<String> = r.log()[n0..].to_vec();
        assert!(new.iter().any(|l| l.starts_with(name)), "dragging {name} wrote nothing: {new:?}");
        assert!(new.iter().all(|l| l.starts_with(name)), "dragging {name} also wrote other parameters: {new:?}");
    }
}

/// Four bands: every tab selects its own band, the third crossover line can be dragged and moves only X-Over 3.
#[test]
fn four_band_tabs_and_the_third_crossover_line() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    for (i, x) in [60.0, 165.0, 270.0, 373.0].into_iter().enumerate() {
        r.click(x, 206.0);
        assert_eq!(r.params.sel_band.load(std::sync::atomic::Ordering::Relaxed), i as i32, "tab {i}");
    }
    // the dotted crossover line at 3.5 kHz: drag it left, only X-Over 3 may change
    let px = satcomp_ui::spectrum::freq_to_x(3500.0, 11.0, 512.0); // viewport x 10, width 514
    let n0 = r.log().len();
    r.drag(px, 60.0, px - 30.0, 60.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(!new.is_empty() && new.iter().all(|l| l.starts_with("X-Over 3")), "{new:?}");
    let last: f32 = new.last().unwrap().rsplit(' ').next().unwrap().parse().unwrap();
    assert!(last < (3500.0 - 200.0) / 19800.0, "dragging left must lower X-Over 3: {new:?}");
}

/// Draw view: the corner pencil opens it with the symmetry tool (odd curve: the left half mirrors the right), the pencil tool
/// draws free, the symmetry tool folds a lopsided curve back onto its odd part, the cross closes the view.
#[test]
fn draw_view_has_a_symmetry_tool_and_a_free_tool() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    let odd = |r: &Rig| {
        let c = r.params.curve_snapshot(0);
        (0..=10).map(|i| (c.eval(i as f64 * 0.1) + c.eval(-(i as f64) * 0.1)).abs()).fold(0.0, f64::max)
    };
    r.click(128.0, 272.0); // corner pencil: opens the draw view (plot at x 10, y 264, 126 square)
    assert_eq!(r.params.curve_snapshot(0).n, 2);
    r.click(99.0, 288.0); // a point at about (0.5, 0.8) on the right half
    let c = r.params.curve_snapshot(0);
    assert_eq!(c.n, 4, "a mirrored pair must appear: {:?}", c.pts());
    assert!(odd(&r) < 1e-6, "symmetry tool keeps the curve odd: {}", odd(&r));
    r.click(114.0, 272.0); // pencil tool: free draw
    r.click(59.0, 331.0); // a lone point on the left half, at about (-0.25, -0.1)
    let n_free = r.params.curve_snapshot(0).n;
    assert_eq!(n_free, 5, "free draw adds a single point");
    assert!(odd(&r) > 0.02, "the curve is lopsided now: {}", odd(&r));
    r.click(100.0, 272.0); // symmetry tool: folds onto the odd part
    assert!(odd(&r) < 1e-6, "folded back onto its odd part: {}", odd(&r));
    let n_fold = r.params.curve_snapshot(0).n;
    r.click(128.0, 272.0); // cross: closes the view
    r.click(99.0, 300.0);
    assert_eq!(r.params.curve_snapshot(0).n, n_fold, "a closed view must not edit the curve");
}

#[test]
fn a_project_restored_after_the_editor_was_built_keeps_its_saved_look() {
    use std::sync::atomic::Ordering::Relaxed;
    // nih-plug builds the editor first and the host restores the project's parameters afterwards
    let mut r = Rig::new();
    assert_eq!((r.st.pal, r.st.light, r.st.face), (0, false, 0));
    r.params.palette_v2.store(5, Relaxed);
    r.params.light.store(true, Relaxed);
    r.params.font.store(2, Relaxed);
    r.st.sync_look(&r.params); // what opening the window does
    assert_eq!((r.st.pal, r.st.light, r.st.face), (5, true, 2));
    for _ in 0..3 {
        r.frame(vec![]);
    }
    assert_eq!(r.params.palette_v2.load(Relaxed), 5, "drawing must not overwrite the restored scheme");
}

/// A sidechain preset comes back keyed: the switch is saved with a preset and applied only when the preset carries it.
#[test]
fn a_preset_saves_and_restores_the_sidechain_switch_but_old_presets_leave_it_alone() {
    // saved from a rig with the switch on
    let mut on = Rig::with_params(|p| p.sc_on = BoolParam::new("Sidechain", true));
    for _ in 0..4 {
        on.frame(vec![]);
    }
    on.st.test_want_capture = true;
    on.frame(vec![]);
    let saved = on.st.test_capture.take().expect("captured");
    assert_eq!(saved.params.get("sc_on"), Some(&1.0), "the switch is stored");
    assert!(!saved.params.contains_key("bypass") && !saved.params.contains_key("gain_match"));

    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    let n0 = r.log().len();
    r.st.test_apply = Some(saved);
    r.frame(vec![]);
    assert!(r.log()[n0..].iter().any(|l| l.starts_with("Sidechain 1.000")), "loading it switches the key on: {:?}", &r.log()[n0..]);
    // an older preset (no sc_on) must not touch the switch, either way
    let n1 = r.log().len();
    r.st.test_apply = Some(crate::presets::Preset::init());
    r.frame(vec![]);
    assert!(!r.log()[n1..].iter().any(|l| l.starts_with("Sidechain")), "old presets / INIT leave the switch alone");
}

/// SOLO and DELTA follow the band tab: selecting another band moves them along instead of leaving the old band soloed.
#[test]
fn solo_and_delta_follow_the_selected_band_tab() {
    let mut r = Rig::with_params(|p| {
        p.lo_solo = BoolParam::new("Lo Solo", true);
        p.lo_delta = BoolParam::new("Lo Delta", true);
    });
    for _ in 0..4 {
        r.frame(vec![]);
    }
    let n0 = r.log().len();
    r.click(167.0, 206.0); // the ML tab
    let new: Vec<String> = r.log()[n0..].to_vec();
    for want in ["Lo Solo 0.000", "ML Solo 1.000", "Lo Delta 0.000", "ML Delta 1.000"] {
        assert!(new.iter().any(|l| l.starts_with(want)), "{want} missing: {new:?}");
    }
    // nothing on: switching tabs writes no solo / delta
    let mut q = Rig::new();
    for _ in 0..4 {
        q.frame(vec![]);
    }
    let n0 = q.log().len();
    q.click(167.0, 206.0);
    assert!(!q.log()[n0..].iter().any(|l| l.contains("Solo") || l.contains("Delta")), "{:?}", &q.log()[n0..]);
}

/// SAT GAIN level match: the SAT readout in a band's plot corner requests a measurement for that band, and the finished
/// result is written to that band's SAT GAIN.
#[test]
fn clicking_the_sat_readout_requests_that_bands_level_match_and_its_result_writes_sat_gain() {
    use std::sync::atomic::Ordering::{Relaxed, Release};
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    r.click(100.0, 382.0); // the SAT readout of the selected (LO) band
    assert!(r.shared.sat_req[0].swap(false, Relaxed), "no match request for band 0");
    assert!(!r.shared.sat_req[1].load(Relaxed), "only the selected band is measured");
    let n0 = r.log().len();
    r.shared.sat_result[0].store(-3.5, Relaxed);
    r.shared.sat_result_new[0].store(true, Release);
    r.frame(vec![]);
    assert!(r.log()[n0..].iter().any(|l| l.starts_with("Lo Sat Gain 0.569")), "{:?}", &r.log()[n0..]);
}

/// Upward compression: the COMP DOWN / UP switch sits on top of the second compressor column (with RANGE under it, KNEE and
/// K.SHAPE as a stacked pair below), it writes the band's UP parameter, and the GR meter is drawn centre-tapped in that mode.
#[test]
fn the_comp_up_switch_writes_the_band_and_the_meter_turns_centre_tapped() {
    use std::sync::atomic::Ordering::Relaxed;
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    let n0 = r.log().len();
    r.click(400.0, 229.0); // DWNWARD -> UPWARD (second column, top row); the mode is 0..2, so UPWARD is 0.500 normalised
    assert!(r.log()[n0..].iter().any(|l| l.starts_with("Lo Comp Mode 0.500")), "{:?}", &r.log()[n0..]);
    // the rest of the cycle, each from a rig that starts in that mode (the fake host logs writes but does not apply them)
    for (from, want) in [(1, "Lo Comp Mode 1.000"), (2, "Lo Comp Mode 0.000")] {
        let mut q = Rig::with_params(|p| p.lo_up = IntParam::new("Lo Comp Mode", from, IntRange::Linear { min: 0, max: 2 }));
        for _ in 0..4 {
            q.frame(vec![]);
        }
        let n = q.log().len();
        q.click(400.0, 229.0);
        assert!(q.log()[n..].iter().any(|l| l.starts_with(want)), "from mode {from}: {:?}", &q.log()[n..]);
    }
    // KNEE and K.SHAPE still reach their parameters from the stacked pair in the second row
    let n0 = r.log().len();
    r.drag(400.0, 273.0, 400.0, 255.0);
    r.drag(400.0, 291.0, 400.0, 273.0);
    let new: Vec<String> = r.log()[n0..].to_vec();
    assert!(new.iter().any(|l| l.starts_with("Lo Knee ")) && new.iter().any(|l| l.starts_with("Lo Knee Shape ")), "{new:?}");

    // the same lift on the meter draws differently with UP on than with it off (centre-tapped, gain colour)
    let frame_hash = |up: bool| {
        let mut q = Rig::with_params(|p| p.lo_up = IntParam::new("Lo Comp Mode", up as i32 * 2, IntRange::Linear { min: 0, max: 2 }));
        q.shared.gl_pk[0].store(6.0, Relaxed);
        for _ in 0..12 {
            q.shared.gl_pk[0].fetch_max(6.0, Relaxed);
            q.frame(vec![]);
        }
        q.hash
    };
    assert_ne!(frame_hash(true), frame_hash(false), "the meter should change with the mode");
    assert_eq!(frame_hash(true), frame_hash(true), "and be deterministic");
}

/// The About signal flow doubles as switches: clicking its LIM block turns the limiter off and a band block writes that band's ON.
/// (The blocks move with the layout, so the test scans the flow area for the clicks that land.)
#[test]
fn the_about_signal_flow_blocks_are_switches() {
    let mut r = Rig::new();
    for _ in 0..4 {
        r.frame(vec![]);
    }
    r.click(80.0, 15.0); // title -> About
    r.frame(vec![]);
    let (mut lim, mut band) = (false, false);
    'scan: for y in (70..170).step_by(4) {
        for x in (150..540).step_by(6) {
            let n0 = r.log().len();
            r.click(x as f32, y as f32);
            for l in r.log()[n0..].iter() {
                lim |= l.starts_with("Limiter");
                band |= l.contains(" On ") || l.ends_with(" On");
            }
            if lim && band {
                break 'scan;
            }
        }
    }
    assert!(lim, "a flow click should switch the limiter: {:?}", r.log());
    assert!(band, "a flow click should switch a band: {:?}", r.log());
    assert_eq!(r.st.app.about, 1, "still on the About page");
}
