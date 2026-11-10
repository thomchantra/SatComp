//! Headless UI regression net. Runs the real editor frame function in an egui context against a fake host, drives it with a
//! scripted mouse (no private state is touched), and hashes the draw commands of every frame. The recorded hashes live in
//! `golden.txt`; any visual or interaction change shows up as a different hash at a named step.
//!
//! Re-record deliberately (only when the UI is *meant* to change):
//!   UI_GOLDEN_RECORD=1 cargo test --release -p satcomp ui_matches_golden_fixture

use super::ui::State;
use crate::params::SatCompParams;
use crate::Shared;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, Event, Modifiers, MouseWheelUnit, PointerButton, Pos2, RawInput, Rect, Vec2};
use std::sync::Arc;

/// Fake host: records every parameter write. (nih-plug keeps its parameter setters crate-private, so values cannot be
/// echoed back; parameters stay at their defaults while the script runs, and the write log is hashed with each frame.)
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
    unsafe fn raw_begin_set_parameter(&self, param: ParamPtr) {
        self.log.lock().push(format!("begin {}", param.name()));
    }
    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
        self.log.lock().push(format!("set {} {:.6}", param.name(), normalized));
    }
    unsafe fn raw_end_set_parameter(&self, param: ParamPtr) {
        self.log.lock().push(format!("end {}", param.name()));
    }
    fn get_state(&self) -> nih_plug::wrapper::state::PluginState {
        nih_plug::wrapper::state::PluginState { version: String::new(), params: Default::default(), fields: Default::default() }
    }
    fn set_state(&self, _state: nih_plug::wrapper::state::PluginState) {}
}

struct Rig {
    ctx: egui::Context,
    host: Arc<FakeHost>,
    params: Arc<SatCompParams>,
    shared: Arc<Shared>,
    st: State,
    t: f64,
    /// screen points per logical unit (window 700x420 = scale 1.25 at ppp 1, scaled with `zoom`)
    k: f32,
    size: Vec2,
    pos: Option<Pos2>,
    down: bool,
    rdown: bool,
    out: Vec<(String, u64)>,
}

fn fnv(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

impl Rig {
    fn new(zoom: f32) -> Self {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(zoom);
        satcomp_ui::init_ctx(&ctx);
        let params = Arc::new(SatCompParams::default());
        let shared = Arc::new(Shared::new());
        let (pal, light) = super::ui::initial_palette(&params);
        let mut st = State::new(pal, light, 0, &params);
        st.themes.clear(); // never depend on the machine's themes.json
        Self {
            ctx,
            host: Arc::new(FakeHost::default()),
            params,
            shared,
            st,
            t: 1.0,
            k: 1.25,
            size: Vec2::new(700.0, 420.0),
            pos: None,
            down: false,
            rdown: false,
            out: Vec::new(),
        }
    }

    /// Deterministic audio-side telemetry so the meters, readouts and spectrum draw something.
    fn feed(&self) {
        use std::sync::atomic::Ordering::Relaxed;
        let n = self.out.len() as f32;
        let w = (n * 0.07).sin().abs();
        let sh = &self.shared;
        sh.opk[0].store(0.15 + 0.8 * w, Relaxed);
        sh.opk[1].store(0.1 + 0.7 * (n * 0.05).cos().abs(), Relaxed);
        sh.gr_pk.store(8.0 * w, Relaxed);
        sh.fms.store(0.002 + 0.05 * w, Relaxed);
        sh.vz_lv.store(0.05 + 0.6 * w, Relaxed);
        sh.sat_vz.store(0.1 + 0.5 * (n * 0.03).sin().abs(), Relaxed);
        sh.mn_lv.store(0.05 + 0.5 * (n * 0.02).cos().abs(), Relaxed);
        sh.diff_disp.store(6.0 * (n * 0.04).sin(), Relaxed);
        sh.sat_db.store(3.0 * (n * 0.06).sin(), Relaxed);
        sh.bg.store(0.8 + 0.4 * w, Relaxed);
        sh.sc_bus.store(self.out.len() % 97 < 60, Relaxed);
        for i in 0..crate::TapRing::LEN {
            let t = (i as f32 + n * 37.0) * 0.001;
            let v = 0.3 * (t * 211.0).sin() + 0.2 * (t * 1733.0).sin() + 0.1 * (t * 7919.0).sin();
            sh.tap.buf[i].store(v.to_bits(), Relaxed);
        }
        sh.tap.w.store(crate::TapRing::LEN * 3, Relaxed);
    }

    fn frame(&mut self, label: &str, events: Vec<Event>, dt: f64) {
        self.feed();
        self.t += dt;
        let raw = RawInput {
            time: Some(self.t),
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            events,
            ..Default::default()
        };
        let setter = ParamSetter::new(&*self.host);
        let (params, shared) = (self.params.clone(), self.shared.clone());
        let st = &mut self.st;
        let full = self.ctx.run(raw, |ctx| super::run_frame(ctx, &setter, st, &params, &shared));
        let writes: Vec<String> = self.host.log.lock().drain(..).collect();
        self.out.push((label.to_string(), fnv(&format!("{:?}|{:?}", full.shapes, writes))));
    }

    fn p(&self, x: f32, y: f32) -> Pos2 {
        Pos2::new(x * self.k, y * self.k)
    }

    fn mv(&mut self, label: &str, x: f32, y: f32) {
        let pos = self.p(x, y);
        self.pos = Some(pos);
        self.frame(label, vec![Event::PointerMoved(pos)], 0.03);
    }

    fn button(&mut self, label: &str, b: PointerButton, pressed: bool) {
        let pos = self.pos.unwrap_or(Pos2::ZERO);
        match b {
            PointerButton::Primary => self.down = pressed,
            _ => self.rdown = pressed,
        }
        self.frame(label, vec![Event::PointerButton { pos, button: b, pressed, modifiers: Modifiers::NONE }], 0.03);
    }

    /// move + press + release at a logical position
    fn click(&mut self, label: &str, x: f32, y: f32) {
        self.mv(&format!("{label}/move"), x, y);
        self.button(&format!("{label}/down"), PointerButton::Primary, true);
        self.button(&format!("{label}/up"), PointerButton::Primary, false);
        self.frame(&format!("{label}/settle"), vec![], 0.05);
    }

    fn rclick(&mut self, label: &str, x: f32, y: f32) {
        self.mv(&format!("{label}/move"), x, y);
        self.button(&format!("{label}/down"), PointerButton::Secondary, true);
        self.button(&format!("{label}/up"), PointerButton::Secondary, false);
        self.frame(&format!("{label}/settle"), vec![], 0.05);
    }

    /// press at (x, y0), move to y1 in `n` steps, release
    fn drag(&mut self, label: &str, x: f32, y0: f32, y1: f32, n: usize) {
        self.mv(&format!("{label}/move"), x, y0);
        self.button(&format!("{label}/down"), PointerButton::Primary, true);
        for i in 1..=n {
            let y = y0 + (y1 - y0) * i as f32 / n as f32;
            self.mv(&format!("{label}/drag{i}"), x, y);
        }
        self.button(&format!("{label}/up"), PointerButton::Primary, false);
        self.frame(&format!("{label}/settle"), vec![], 0.05);
    }

    fn wheel(&mut self, label: &str, x: f32, y: f32, dy: f32) {
        self.mv(&format!("{label}/move"), x, y);
        let pos = self.pos.unwrap_or(Pos2::ZERO);
        let _ = pos;
        self.frame(
            &format!("{label}/wheel"),
            vec![Event::MouseWheel { unit: MouseWheelUnit::Point, delta: Vec2::new(0.0, dy), modifiers: Modifiers::NONE }],
            0.03,
        );
        self.frame(&format!("{label}/settle"), vec![], 0.05);
    }

    fn idle(&mut self, label: &str, n: usize, dt: f64) {
        for i in 0..n {
            self.frame(&format!("{label}{i}"), vec![], dt);
        }
    }
}

fn script() -> Vec<(String, u64)> {
    let mut r = Rig::new(1.0);
    r.idle("boot", 4, 0.016);

    // sliders: drag, wheel, double-click reset
    r.drag("drive_drag", 41.0, 215.0, 190.0, 6);
    r.wheel("thresh_wheel", 183.0, 215.0, 20.0);
    r.wheel("thresh_wheel_dn", 183.0, 215.0, -20.0);
    r.drag("ratio_drag", 251.0, 215.0, 240.0, 5);
    r.click("drive_dbl_a", 41.0, 215.0);
    r.click("drive_dbl_b", 41.0, 215.0);
    r.drag("hp_drag", 41.0, 250.0, 225.0, 4);
    r.drag("lp_drag", 41.0, 270.0, 250.0, 4);
    r.drag("makeup_drag", 507.0, 215.0, 200.0, 4);
    r.idle("idle_a", 10, 0.05);

    // saturator shape selector: step, then open its menu and pick an item
    r.click("shape_next", 132.0, 42.0);
    r.click("shape_next2", 132.0, 42.0);
    r.click("shape_menu_open", 75.0, 42.0);
    r.idle("shape_menu_idle", 2, 0.05);
    r.click("shape_menu_pick", 75.0, 92.0);
    r.click("shape_prev", 18.0, 42.0);

    // oversampling, sidechain, bypass, delta, gain match
    r.click("os_x2", 192.0, 42.0);
    r.click("os_x4", 232.0, 42.0);
    r.click("os_x1", 172.0, 42.0);
    r.click("sidechain_on", 314.0, 42.0);
    r.idle("sidechain_idle", 3, 0.05);
    r.drag("sc_gain_drag", 190.0, 175.0, 160.0, 3);
    r.click("sidechain_off", 314.0, 42.0);
    r.click("delta_on", 460.0, 42.0);
    r.click("delta_off", 460.0, 42.0);
    r.click("bypass_on", 520.0, 42.0);
    r.idle("bypass_idle", 3, 0.05);
    r.click("bypass_off", 520.0, 42.0);
    r.click("gain_match", 507.0, 135.0);
    r.idle("gain_match_idle", 6, 0.1);
    r.click("gain_match_off", 507.0, 135.0);

    // sat plot: pencil (custom curve draw view), draw points, leave
    r.click("pencil_on", 130.0, 66.0);
    r.idle("pencil_idle", 3, 0.05);
    r.click("curve_pt_add", 60.0, 100.0);
    r.drag("curve_pt_drag", 80.0, 120.0, 90.0, 4);
    r.click("curve_pt_add2", 110.0, 80.0);
    r.click("pencil_off", 130.0, 66.0);

    // scope icon: on, drag the view, off
    r.click("scope_on", 410.0, 64.0);
    r.idle("scope_idle", 4, 0.05);
    r.drag("scope_drag", 280.0, 110.0, 130.0, 4);
    r.click("scope_off", 410.0, 64.0);

    // compressor plot interaction
    r.drag("ccurve_drag", 200.0, 120.0, 100.0, 4);

    // EQ page
    r.click("eq_on", 387.0, 42.0);
    r.idle("eq_idle", 4, 0.05);
    r.drag("eq_band_drag", 190.0, 130.0, 100.0, 5);
    r.drag("eq_band2_drag", 260.0, 130.0, 150.0, 5);
    r.wheel("eq_band_wheel", 190.0, 100.0, 20.0);
    r.click("eq_band_dbl_a", 190.0, 100.0);
    r.click("eq_band_dbl_b", 190.0, 100.0);
    r.click("eq_type_icon", 160.0, 207.0);
    r.click("eq_type_icon2", 200.0, 220.0);
    r.drag("eq_freq_cell", 183.0, 260.0, 240.0, 4);
    r.drag("eq_gain_cell", 183.0, 300.0, 280.0, 4);
    r.drag("eq_tilt_cell", 387.0, 260.0, 240.0, 4);
    r.click("eq_stage_toggle", 300.0, 177.0);
    r.idle("eq_stage_idle", 3, 0.05);
    r.click("eq_stage_toggle2", 160.0, 177.0);
    r.click("eq_module_byp", 370.0, 42.0);
    r.idle("eq_byp_idle", 2, 0.05);
    r.click("eq_module_byp_off", 370.0, 42.0);
    r.click("eq_off", 400.0, 42.0);

    // presets: next / previous (applies factory presets through the host)
    r.click("preset_next", 332.0, 16.0);
    r.idle("preset_next_idle", 3, 0.05);
    r.click("preset_next2", 332.0, 16.0);
    r.click("preset_prev", 332.0 - 25.0, 16.0);
    r.idle("preset_idle", 8, 0.1);

    // colour scheme: single click cycles (after the double-click window), right-click menu, double-click toggles light
    r.click("logo_single", 20.0, 15.0);
    r.idle("logo_single_wait", 8, 0.05);
    r.rclick("logo_menu", 20.0, 15.0);
    r.idle("logo_menu_idle", 2, 0.05);
    r.click("logo_menu_pick", 60.0, 80.0);
    r.click("logo_dbl_a", 20.0, 15.0);
    r.click("logo_dbl_b", 20.0, 15.0);
    r.idle("light_idle", 3, 0.05);
    r.click("logo_dbl2_a", 20.0, 15.0);
    r.click("logo_dbl2_b", 20.0, 15.0);

    // About / Appearance / Changelog
    r.click("about_open", 80.0, 15.0);
    r.idle("about_idle", 3, 0.05);
    r.wheel("about_scroll", 200.0, 250.0, 40.0);
    r.wheel("about_scroll2", 200.0, 250.0, 40.0);
    r.click("about_tab_appearance", 180.0, 42.0);
    r.idle("appearance_idle", 2, 0.05);
    r.wheel("appearance_scroll", 100.0, 150.0, 40.0);
    r.click("appearance_scheme_pick", 80.0, 100.0);
    r.click("appearance_font_pick", 330.0, 100.0);
    r.click("appearance_font_pick2", 330.0, 140.0);
    r.click("about_tab_changelog", 300.0, 42.0);
    r.idle("changelog_idle", 2, 0.05);
    r.wheel("changelog_scroll", 200.0, 200.0, 40.0);
    r.drag("changelog_thumb", 414.0, 100.0, 200.0, 4);
    r.click("about_tab_about", 60.0, 42.0);
    r.click("about_close", 387.0, 42.0);
    r.idle("closed_idle", 3, 0.05);

    // window scale button + a different pixel density
    r.click("scale_menu_open", 397.0, 16.0);
    r.idle("scale_menu_idle", 2, 0.05);
    r.click("scale_menu_close", 300.0, 300.0);

    let mut r2 = Rig::new(2.0);
    r2.size = Vec2::new(1400.0 / 2.0 * 2.0, 840.0);
    r2.k = 2.5;
    r2.idle("hidpi_boot", 4, 0.016);
    r2.drag("hidpi_drive", 41.0, 215.0, 190.0, 4);
    r2.click("hidpi_eq_on", 387.0, 42.0);
    r2.idle("hidpi_idle", 3, 0.05);

    let mut out = r.out;
    out.extend(r2.out.into_iter().map(|(l, h)| (format!("hidpi:{l}"), h)));
    out
}

#[test]
fn ui_matches_golden_fixture() {
    let got = script();
    let text: String = got.iter().map(|(l, h)| format!("{l} {h:016x}\n")).collect();
    if let Ok(path) = std::env::var("UI_GOLDEN_OUT") {
        std::fs::write(path, &text).unwrap();
        return;
    }
    let fixture = include_str!("golden.txt");
    let want: Vec<(&str, &str)> = fixture.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).filter_map(|l| l.split_once(' ')).collect();
    assert_eq!(want.len(), got.len(), "frame count changed: fixture {} vs now {}", want.len(), got.len());
    let mut bad = Vec::new();
    for ((wl, wh), (gl, gh)) in want.iter().zip(&got) {
        if wl != gl || *wh != format!("{gh:016x}") {
            bad.push(format!("{gl} (fixture {wl} {wh}, now {gh:016x})"));
        }
    }
    assert!(bad.is_empty(), "{} frames differ; first: {}", bad.len(), bad.first().unwrap());
}

#[test]
fn ui_script_is_deterministic() {
    let a = script();
    let b = script();
    assert_eq!(a, b);
}
