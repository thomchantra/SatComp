//! plugbench: an offline host that loads CLAP (SatComp family) and VST2 (Cockos ReaPlugs) plug-ins, pushes real audio through them
//! block by block on ONE thread (like a host's audio thread, flush-to-zero set like REAPER) and times every block.
//!
//!   plugbench list <path>                       parameter names of a plug-in
//!   plugbench run <audio.wav> [block] [secs]    the benchmark suite (see `suite()`), markdown table on stdout
//!
//! Only macOS / Unix (dlopen).

use clap_sys::audio_buffer::clap_audio_buffer;
use clap_sys::entry::clap_plugin_entry;
use clap_sys::events::*;
use clap_sys::ext::audio_ports::*;
use clap_sys::ext::params::*;
use clap_sys::factory::plugin_factory::*;
use clap_sys::host::clap_host;
use clap_sys::plugin::clap_plugin;
use clap_sys::process::clap_process;
use clap_sys::version::CLAP_VERSION;
use std::ffi::{c_char, c_void, CStr, CString};
use std::time::Instant;

extern "C" {
    fn pthread_set_qos_class_self_np(qos: u32, relative_priority: i32) -> i32;
    fn dlopen(path: *const c_char, flags: i32) -> *mut c_void;
    fn dlsym(h: *mut c_void, name: *const c_char) -> *mut c_void;
}

fn sym(h: *mut c_void, name: &str) -> *mut c_void {
    let c = CString::new(name).unwrap();
    unsafe { dlsym(h, c.as_ptr()) }
}

fn open_lib(path: &str) -> *mut c_void {
    let c = CString::new(path).unwrap();
    let h = unsafe { dlopen(c.as_ptr(), 2) };
    assert!(!h.is_null(), "cannot load {path}");
    h
}

/// Flush denormals to zero like REAPER's audio threads (aarch64: FPCR.FZ; x86_64: MXCSR FTZ + DAZ).
fn set_ftz() {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let mut f: u64;
        std::arch::asm!("mrs {}, fpcr", out(reg) f);
        f |= 1 << 24;
        std::arch::asm!("msr fpcr, {}", in(reg) f);
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        #[allow(deprecated)]
        {
            let m = std::arch::x86_64::_mm_getcsr();
            std::arch::x86_64::_mm_setcsr(m | 0x8040);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------- plug-in trait

trait Plug {
    fn name(&self) -> String;
    fn params(&self) -> Vec<String>;
    /// set a parameter by (a part of) its name to a plain value inside its [min, max]; returns whether it was found
    fn set(&mut self, name: &str, plain: f64, min: f64, max: f64) -> bool;
    /// set a parameter so that its DISPLAY text parses to `target` (VST2; bisection)
    fn set_display(&mut self, _name: &str, _target: f64) -> bool {
        false
    }
    /// copy the input block into the plug-in's own buffers (not timed)
    fn prepare(&mut self, inp: &[Vec<f32>; 2], n: usize);
    /// the plug-in's processing of one block (timed)
    fn run(&mut self, n: usize);
    /// copy the result out (not timed)
    fn collect(&mut self, out: &mut [Vec<f32>; 2], n: usize);
    fn latency(&self) -> u32 {
        0
    }
}

// ---------------------------------------------------------------------------------------------------------------- CLAP

struct Events {
    list: Vec<clap_event_param_value>,
}

unsafe extern "C" fn ev_size(l: *const clap_input_events) -> u32 {
    let e = &*((*l).ctx as *const Events);
    e.list.len() as u32
}
unsafe extern "C" fn ev_get(l: *const clap_input_events, i: u32) -> *const clap_event_header {
    let e = &*((*l).ctx as *const Events);
    &e.list[i as usize].header
}
unsafe extern "C" fn ev_push(_: *const clap_output_events, _: *const clap_event_header) -> bool {
    true
}
unsafe extern "C" fn host_ext(_: *const clap_host, _: *const c_char) -> *const c_void {
    std::ptr::null()
}
unsafe extern "C" fn host_nop(_: *const clap_host) {}

struct Clap {
    plug: *const clap_plugin,
    sr: f64,
    pending: Box<Events>,
    names: Vec<(String, u32, f64, f64)>, // name, id, min, max
    ports_in: Vec<u32>,
    aux: Vec<Vec<f32>>,
    bin: [Vec<f32>; 2],
    bout: [Vec<f32>; 2],
    steady: i64,
    desc: String,
    _host: Box<clap_host>,
}

impl Clap {
    fn load(bundle: &str, sr: f64, max_block: u32) -> Self {
        let stem = std::path::Path::new(bundle).file_stem().unwrap().to_string_lossy().to_string();
        let lib = open_lib(&format!("{bundle}/Contents/MacOS/{stem}"));
        let entry = sym(lib, "clap_entry") as *const clap_plugin_entry;
        assert!(!entry.is_null(), "no clap_entry in {bundle}");
        unsafe {
            let path = CString::new(bundle).unwrap();
            assert!(((*entry).init.unwrap())(path.as_ptr()));
            let fac = ((*entry).get_factory.unwrap())(CLAP_PLUGIN_FACTORY_ID.as_ptr()) as *const clap_plugin_factory;
            let d = ((*fac).get_plugin_descriptor.unwrap())(fac, 0);
            let id = (*d).id;
            let host = Box::new(clap_host {
                clap_version: CLAP_VERSION,
                host_data: std::ptr::null_mut(),
                name: b"plugbench\0".as_ptr() as *const c_char,
                vendor: b"plugbench\0".as_ptr() as *const c_char,
                url: b"\0".as_ptr() as *const c_char,
                version: b"1\0".as_ptr() as *const c_char,
                get_extension: Some(host_ext),
                request_restart: Some(host_nop),
                request_process: Some(host_nop),
                request_callback: Some(host_nop),
            });
            let plug = ((*fac).create_plugin.unwrap())(fac, &*host, id);
            assert!(!plug.is_null());
            assert!(((*plug).init.unwrap())(plug));
            assert!(((*plug).activate.unwrap())(plug, sr, 1, max_block));
            assert!(((*plug).start_processing.unwrap())(plug));
            let pe = ((*plug).get_extension.unwrap())(plug, CLAP_EXT_PARAMS.as_ptr()) as *const clap_plugin_params;
            let mut names = Vec::new();
            if !pe.is_null() {
                for i in 0..((*pe).count.unwrap())(plug) {
                    let mut info: clap_param_info = std::mem::zeroed();
                    if ((*pe).get_info.unwrap())(plug, i, &mut info) {
                        names.push((CStr::from_ptr(info.name.as_ptr()).to_string_lossy().to_string(), info.id, info.min_value, info.max_value));
                    }
                }
            }
            let ap = ((*plug).get_extension.unwrap())(plug, CLAP_EXT_AUDIO_PORTS.as_ptr()) as *const clap_plugin_audio_ports;
            let mut ports_in = Vec::new();
            if !ap.is_null() {
                for i in 0..((*ap).count.unwrap())(plug, true) {
                    let mut info: clap_audio_port_info = std::mem::zeroed();
                    if ((*ap).get.unwrap())(plug, i, true, &mut info) {
                        ports_in.push(info.channel_count);
                    }
                }
            }
            let desc = CStr::from_ptr((*d).name).to_string_lossy().to_string();
            let aux = (0..ports_in.len().saturating_sub(1) * 2).map(|_| vec![0.0f32; max_block as usize]).collect();
            Clap { plug, sr, pending: Box::new(Events { list: Vec::new() }), names, ports_in, aux, bin: [vec![0.0; max_block as usize], vec![0.0; max_block as usize]], bout: [vec![0.0; max_block as usize], vec![0.0; max_block as usize]], steady: 0, desc, _host: host }
        }
    }
}

impl Plug for Clap {
    fn name(&self) -> String {
        self.desc.clone()
    }
    fn params(&self) -> Vec<String> {
        self.names.iter().map(|n| format!("{} (min {} max {})", n.0, n.2, n.3)).collect()
    }
    fn set(&mut self, name: &str, plain: f64, min: f64, max: f64) -> bool {
        let Some((_, id, lo, hi)) = self.names.iter().find(|n| n.0 == name).cloned() else { return false };
        let norm = ((plain - min) / (max - min)).clamp(0.0, 1.0);
        let ev = clap_event_param_value {
            header: clap_event_header { size: std::mem::size_of::<clap_event_param_value>() as u32, time: 0, space_id: CLAP_CORE_EVENT_SPACE_ID, type_: CLAP_EVENT_PARAM_VALUE, flags: 0 },
            param_id: id,
            cookie: std::ptr::null_mut(),
            note_id: -1,
            port_index: -1,
            channel: -1,
            key: -1,
            value: lo + norm * (hi - lo),
        };
        self.pending.list.push(ev);
        true
    }
    fn prepare(&mut self, inp: &[Vec<f32>; 2], n: usize) {
        self.bin[0][..n].copy_from_slice(&inp[0][..n]);
        self.bin[1][..n].copy_from_slice(&inp[1][..n]);
    }
    fn collect(&mut self, out: &mut [Vec<f32>; 2], n: usize) {
        out[0][..n].copy_from_slice(&self.bout[0][..n]);
        out[1][..n].copy_from_slice(&self.bout[1][..n]);
    }
    fn run(&mut self, n: usize) {
        unsafe {
            let mut in_ptrs = [self.bin[0].as_mut_ptr(), self.bin[1].as_mut_ptr()];
            let mut out_ptrs = [self.bout[0].as_mut_ptr(), self.bout[1].as_mut_ptr()];            let mut aux_ptrs: Vec<[*mut f32; 2]> = (0..self.aux.len() / 2).map(|k| [self.aux[2 * k].as_ptr() as *mut f32, self.aux[2 * k + 1].as_ptr() as *mut f32]).collect();
            let mut ins = vec![clap_audio_buffer { data32: in_ptrs.as_mut_ptr(), data64: std::ptr::null_mut(), channel_count: 2, latency: 0, constant_mask: 0 }];
            for p in aux_ptrs.iter_mut() {
                ins.push(clap_audio_buffer { data32: p.as_mut_ptr(), data64: std::ptr::null_mut(), channel_count: 2, latency: 0, constant_mask: 0 });
            }
            let mut outs = [clap_audio_buffer { data32: out_ptrs.as_mut_ptr(), data64: std::ptr::null_mut(), channel_count: 2, latency: 0, constant_mask: 0 }];
            let in_ev = clap_input_events { ctx: &*self.pending as *const Events as *mut c_void, size: Some(ev_size), get: Some(ev_get) };
            let out_ev = clap_output_events { ctx: std::ptr::null_mut(), try_push: Some(ev_push) };
            let pr = clap_process {
                steady_time: self.steady,
                frames_count: n as u32,
                transport: std::ptr::null(),
                audio_inputs: ins.as_ptr(),
                audio_outputs: outs.as_mut_ptr(),
                audio_inputs_count: ins.len() as u32,
                audio_outputs_count: 1,
                in_events: &in_ev,
                out_events: &out_ev,
            };
            ((*self.plug).process.unwrap())(self.plug, &pr);
            self.pending.list.clear();
            self.steady += n as i64;
            let _ = (self.sr, &self.ports_in);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------- VST2

#[repr(C)]
struct AEffect {
    magic: i32,
    dispatcher: extern "C" fn(*mut AEffect, i32, i32, isize, *mut c_void, f32) -> isize,
    process: *const c_void,
    set_parameter: extern "C" fn(*mut AEffect, i32, f32),
    get_parameter: extern "C" fn(*mut AEffect, i32) -> f32,
    num_programs: i32,
    num_params: i32,
    num_inputs: i32,
    num_outputs: i32,
    flags: i32,
    resvd1: isize,
    resvd2: isize,
    initial_delay: i32,
    real_qualities: i32,
    off_qualities: i32,
    io_ratio: f32,
    object: *mut c_void,
    user: *mut c_void,
    unique_id: i32,
    version: i32,
    process_replacing: extern "C" fn(*mut AEffect, *const *const f32, *mut *mut f32, i32),
    process_double: *const c_void,
    future: [u8; 56],
}

static SAMPLE_RATE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(44100);

/// a stand-in for every SWELL (REAPER's UI layer) function: this host has no UI, so none of them is ever called for real
extern "C" fn swell_stub() -> isize {
    0
}

static SWELL_LOG: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

extern "C" fn vst_host(_: *mut AEffect, opcode: i32, index: i32, _: isize, ptr: *mut c_void, _: f32) -> isize {
    // the ReaPlugs on macOS ask the host for every SWELL function by name (opcode 0xdeadbeef, index 0xdeadf00d, name in ptr) and refuse
    // to load when one is missing or the version check fails
    if opcode as u32 == 0xdeadbeef && index as u32 == 0xdeadf00d && !ptr.is_null() {
        let name = unsafe { CStr::from_ptr(ptr as *const c_char) }.to_string_lossy().to_string();
        if std::env::var_os("PLUGBENCH_SWELL").is_some() && SWELL_LOG.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 12 {
            eprintln!("  swell request: {name}");
        }
        return swell_stub as extern "C" fn() -> isize as usize as isize;
    }
    match opcode {
        1 => 2400,                                                     // audioMasterVersion
        16 => SAMPLE_RATE.load(std::sync::atomic::Ordering::Relaxed) as isize, // audioMasterGetSampleRate
        17 => 512,                                                     // audioMasterGetBlockSize
        23 => 2,                                                       // audioMasterGetCurrentProcessLevel: realtime
        _ => 0,
    }
}

struct Vst2 {
    e: *mut AEffect,
    name: String,
    /// double-precision buffers (the ReaPlugs process doubles) and silence for inputs beyond the first two (a sidechain)
    din: [Vec<f64>; 2],
    dout: [Vec<f64>; 2],
    dzero: Vec<f64>,
    dspare: Vec<Vec<f64>>,
}

impl Vst2 {
    fn dispatch(&self, op: i32, idx: i32, val: isize, ptr: *mut c_void, opt: f32) -> isize {
        unsafe { ((*self.e).dispatcher)(self.e, op, idx, val, ptr, opt) }
    }
    fn text(&self, op: i32, idx: i32) -> String {
        let mut buf = [0u8; 256];
        self.dispatch(op, idx, 0, buf.as_mut_ptr() as *mut c_void, 0.0);
        let end = buf.iter().position(|&b| b == 0).unwrap_or(0);
        String::from_utf8_lossy(&buf[..end]).to_string()
    }
    fn load(path: &str, sr: f64, max_block: i32) -> Self {
        SAMPLE_RATE.store(sr as i64, std::sync::atomic::Ordering::Relaxed);
        let lib = open_lib(path);
        let mut f = sym(lib, "VSTPluginMain");
        if f.is_null() {
            f = sym(lib, "main_macho");
        }
        assert!(!f.is_null(), "no VST2 entry in {path}");
        let entry: extern "C" fn(extern "C" fn(*mut AEffect, i32, i32, isize, *mut c_void, f32) -> isize) -> *mut AEffect = unsafe { std::mem::transmute(f) };
        let e = entry(vst_host);
        assert!(!e.is_null() && unsafe { (*e).magic } == 0x56737450, "not a VST2 effect");
        let mb = max_block as usize;
        let mut v = Vst2 { e, name: String::new(), din: [vec![0.0; mb], vec![0.0; mb]], dout: [vec![0.0; mb], vec![0.0; mb]], dzero: vec![0.0; mb], dspare: (0..6).map(|_| vec![0.0; mb]).collect() };
        v.dispatch(0, 0, 0, std::ptr::null_mut(), 0.0); // effOpen
        v.dispatch(10, 0, 0, std::ptr::null_mut(), sr as f32); // effSetSampleRate
        v.dispatch(11, 0, max_block as isize, std::ptr::null_mut(), 0.0); // effSetBlockSize
        v.dispatch(12, 0, 1, std::ptr::null_mut(), 0.0); // effMainsChanged on
        v.dispatch(71, 0, 0, std::ptr::null_mut(), 0.0); // effStartProcess
        v.name = v.text(45, 0); // effGetEffectName
        if std::env::var_os("PLUGBENCH_INFO").is_some() {
            let (ni, no, fl, dl) = unsafe { ((*e).num_inputs, (*e).num_outputs, (*e).flags, (*e).initial_delay) };
            eprintln!("  vst2: {} in, {} out, flags {:#x}, params {}, latency {}", ni, no, fl, v.n_params(), dl);
        }
        v
    }
    fn n_params(&self) -> i32 {
        unsafe { (*self.e).num_params }
    }
    fn find(&self, name: &str) -> Option<i32> {
        if let Some(n) = name.strip_prefix('#') {
            return n.parse().ok().filter(|&i| i < self.n_params());
        }
        (0..self.n_params()).find(|&i| self.text(8, i).to_lowercase().contains(&name.to_lowercase()))
    }
}

impl Plug for Vst2 {
    fn name(&self) -> String {
        self.name.clone()
    }
    fn params(&self) -> Vec<String> {
        (0..self.n_params()).map(|i| format!("#{i} {} = {} {}", self.text(8, i), self.text(7, i), self.text(6, i))).collect()
    }
    fn set(&mut self, name: &str, plain: f64, min: f64, max: f64) -> bool {
        let Some(i) = self.find(name) else { return false };
        unsafe { ((*self.e).set_parameter)(self.e, i, ((plain - min) / (max - min)).clamp(0.0, 1.0) as f32) };
        true
    }
    fn set_display(&mut self, name: &str, target: f64) -> bool {
        let Some(i) = self.find(name) else { return false };
        let parse = |s: &str| s.split_whitespace().next().and_then(|t| t.trim_end_matches(|c: char| !c.is_ascii_digit()).parse::<f64>().ok());
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        let probe = |v: f32, me: &Vst2| {
            unsafe { ((*me.e).set_parameter)(me.e, i, v) };
            parse(&me.text(7, i))
        };
        let (a, b) = (probe(0.02, self), probe(0.98, self));
        let (Some(a), Some(b)) = (a, b) else { return false };
        let up = b > a;
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            let Some(d) = probe(mid, self) else { return false };
            if (d < target) == up {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        probe(0.5 * (lo + hi), self);
        true
    }
    fn prepare(&mut self, inp: &[Vec<f32>; 2], n: usize) {
        for ch in 0..2 {
            for i in 0..n {
                self.din[ch][i] = inp[ch][i] as f64;
            }
        }
    }
    fn collect(&mut self, out: &mut [Vec<f32>; 2], n: usize) {
        for ch in 0..2 {
            for i in 0..n {
                out[ch][i] = self.dout[ch][i] as f32;
            }
        }
    }
    /// The ReaPlugs do their DSP in `processDoubleReplacing` (the float entry points are empty stubs; REAPER itself calls the double one).
    fn run(&mut self, n: usize) {
        let n_in = unsafe { (*self.e).num_inputs.max(2) as usize };
        let mut ins: Vec<*const f64> = vec![self.dzero.as_ptr(); n_in];
        ins[0] = self.din[0].as_ptr();
        ins[1] = self.din[1].as_ptr();
        let mut outs: Vec<*mut f64> = vec![self.dout[0].as_mut_ptr(), self.dout[1].as_mut_ptr()];
        let n_out = unsafe { (*self.e).num_outputs.max(2) as usize };
        for k in 2..n_out {
            outs.push(self.dspare[k - 2].as_mut_ptr());
        }
        let f: extern "C" fn(*mut AEffect, *const *const f64, *mut *mut f64, i32) = unsafe { std::mem::transmute((*self.e).process_double) };
        f(self.e, ins.as_ptr(), outs.as_mut_ptr(), n as i32);
    }
    fn latency(&self) -> u32 {
        unsafe { (*self.e).initial_delay as u32 }
    }
}

// ---------------------------------------------------------------------------------------------------------------- audio

fn read_wav(path: &str) -> (Vec<f32>, Vec<f32>, f64) {
    let b = std::fs::read(path).expect("wav");
    assert!(&b[0..4] == b"RIFF" && &b[8..12] == b"WAVE");
    let (mut p, mut ch, mut bits, mut sr, mut data) = (12, 2usize, 16usize, 44100u32, &b[0..0]);
    while p + 8 <= b.len() {
        let id = &b[p..p + 4];
        let sz = u32::from_le_bytes(b[p + 4..p + 8].try_into().unwrap()) as usize;
        let body = &b[p + 8..(p + 8 + sz).min(b.len())];
        if id == b"fmt " {
            ch = u16::from_le_bytes(body[2..4].try_into().unwrap()) as usize;
            sr = u32::from_le_bytes(body[4..8].try_into().unwrap());
            bits = u16::from_le_bytes(body[14..16].try_into().unwrap()) as usize;
        } else if id == b"data" {
            data = body;
        }
        p += 8 + sz + (sz & 1);
    }
    assert!(bits == 16, "16-bit only");
    let frames = data.len() / (2 * ch);
    let (mut l, mut r) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    for i in 0..frames {
        let s = |c: usize| i16::from_le_bytes(data[(i * ch + c.min(ch - 1)) * 2..(i * ch + c.min(ch - 1)) * 2 + 2].try_into().unwrap()) as f32 / 32768.0;
        l.push(s(0));
        r.push(s(1));
    }
    (l, r, sr as f64)
}

// ---------------------------------------------------------------------------------------------------------------- suite

enum Kind {
    Clap,
    Vst2,
}

struct Case {
    label: &'static str,
    kind: Kind,
    path: &'static str,
    /// (name, plain, min, max) for CLAP; display targets for VST2: ("name", target, f64::NAN, f64::NAN)
    set: Vec<(String, f64, f64, f64)>,
}

type Set = Vec<(String, f64, f64, f64)>;

fn clap(label: &'static str, path: &'static str, set: Set) -> Case {
    Case { label, kind: Kind::Clap, path, set }
}
fn vst(label: &'static str, path: &'static str, set: Set) -> Case {
    Case { label, kind: Kind::Vst2, path, set }
}

/// CLAP: a plain value with its range
fn c(name: &str, plain: f64, min: f64, max: f64) -> (String, f64, f64, f64) {
    (name.to_string(), plain, min, max)
}
/// VST2: a parameter (by index) set so that its display text reads `target`
fn d(index: usize, target: f64) -> (String, f64, f64, f64) {
    (format!("#{index}"), target, f64::NAN, f64::NAN)
}

/// SatComp's compressor at a typical bus setting: -20 dB threshold, 4:1, 10 ms / 150 ms
fn sc_comp() -> Set {
    vec![c("Thresh", -20.0, -40.0, 0.0), c("Ratio", 4.0, 1.0, 20.0), c("Attack", 10.0, 0.05, 100.0), c("Release", 150.0, 5.0, 2000.0)]
}

/// all four SatXComp bands compressing at -20 dB, 4:1, 10 ms / 150 ms
fn xc_comp() -> Set {
    let mut v = Vec::new();
    for b in ["Lo", "ML", "MH", "Hi"] {
        v.push(c(&format!("{b} Thresh"), -20.0, -40.0, 0.0));
        v.push(c(&format!("{b} Ratio"), 4.0, 1.0, 20.0));
        v.push(c(&format!("{b} Attack"), 10.0, 0.05, 100.0));
        v.push(c(&format!("{b} Release"), 150.0, 5.0, 2000.0));
    }
    v
}

fn xc_drive(drive: f64) -> Set {
    ["Lo", "ML", "MH", "Hi"].iter().map(|b| c(&format!("{b} Drive"), drive, 0.0, 100.0)).collect()
}

fn cat(parts: Vec<Set>) -> Set {
    parts.into_iter().flatten().collect()
}

fn suite() -> Vec<Case> {
    let b = "../../target/bundled";
    let _ = b;
    let sc = "../../target/bundled/satcomp.clap";
    let xc = "../../target/bundled/satxcomp.clap";
    let sl = "../../target/bundled/satlim.clap";
    let rc = "../../../vst/reacomp.vst.dylib";
    let rx = "../../../vst/reaxcomp.vst.dylib";
    vec![
        // ---- single-band compressors
        vst("ReaComp (defaults: no reduction)", rc, vec![]),
        vst("ReaComp (-20 dB, 4:1, 10 / 150 ms)", rc, vec![d(0, -20.0), d(1, 4.0), d(2, 10.0), d(3, 150.0)]),
        clap("SatComp, compressor only (limiter off)", sc, cat(vec![sc_comp(), vec![c("Limiter", 0.0, 0.0, 1.0)]])),
        clap("SatComp, as shipped (limiter on)", sc, sc_comp()),
        clap("SatComp + saturator drive 30 %, x1", sc, cat(vec![sc_comp(), vec![c("Drive", 30.0, 0.0, 100.0)]])),
        clap("SatComp + saturator drive 30 %, x4 oversampling", sc, cat(vec![sc_comp(), vec![c("Drive", 30.0, 0.0, 100.0), c("Oversampling", 2.0, 0.0, 2.0)]])),
        // ---- multiband
        vst("ReaXcomp (defaults, 4 bands)", rx, vec![]),
        clap("SatXComp (defaults, 4 bands)", xc, vec![]),
        clap("SatXComp, 4 bands compressing", xc, xc_comp()),
        clap("SatXComp, compressing + drive 30 % x1", xc, cat(vec![xc_comp(), xc_drive(30.0)])),
        clap("SatXComp, compressing + drive 30 % x4", xc, cat(vec![xc_comp(), xc_drive(30.0), vec![c("Sat Oversampling", 2.0, 0.0, 2.0)]])),
        // ---- limiter
        clap("SatLim (defaults)", sl, vec![]),
        clap("SatLim, limiting (-6 dB threshold, -1 dB ceiling)", sl, vec![c("Threshold", -6.0, -30.0, 0.0), c("Ceiling", -1.0, -24.0, 0.0)]),
        clap("SatLim, limiting + drive 30 %, x4", sl, vec![c("Threshold", -6.0, -30.0, 0.0), c("Ceiling", -1.0, -24.0, 0.0), c("Drive", 30.0, 0.0, 100.0), c("Oversampling", 2.0, 0.0, 3.0)]),
    ]
}

fn make(c: &Case, sr: f64, block: u32) -> Box<dyn Plug> {
    let mut p: Box<dyn Plug> = match c.kind {
        Kind::Clap => Box::new(Clap::load(c.path, sr, block)),
        Kind::Vst2 => Box::new(Vst2::load(c.path, sr, block as i32)),
    };
    for (n, a, lo, hi) in &c.set {
        let (a, lo, hi) = (*a, *lo, *hi);
        let ok = if lo.is_nan() { p.set_display(n, a) } else { p.set(n, a, lo, hi) };
        if !ok {
            eprintln!("  ! {}: parameter '{}' not found", c.label, n);
        }
    }
    p
}

struct Stat {
    median_us: f64,
    p99_us: f64,
    budget_us: f64,
    out_rms_db: f64,
    out_peak_db: f64,
}

fn run_case(c: &Case, copies: usize, wav: &(Vec<f32>, Vec<f32>, f64), block: usize, secs: f64) -> Stat {
    let sr = wav.2;
    let mut plugs: Vec<Box<dyn Plug>> = (0..copies).map(|_| make(c, sr, block as u32)).collect();
    let mut inp = [vec![0.0f32; block], vec![0.0f32; block]];
    let mut outs: Vec<[Vec<f32>; 2]> = (0..copies).map(|_| [vec![0.0f32; block], vec![0.0f32; block]]).collect();
    let total = (secs * sr) as usize / block * block;
    let warm = (3.0 * sr) as usize / block * block;
    let len = wav.0.len();
    let mut times: Vec<f64> = Vec::with_capacity(total / block);
    let (mut sum_sq, mut peak, mut cnt) = (0.0f64, 0.0f32, 0usize);
    let mut pos = 0usize;
    for blk in 0..((warm + total) / block) {
        for i in 0..block {
            inp[0][i] = wav.0[(pos + i) % len];
            inp[1][i] = wav.1[(pos + i) % len];
        }
        pos += block;
        for p in plugs.iter_mut() {
            p.prepare(&inp, block);
        }
        let t0 = Instant::now();
        for p in plugs.iter_mut() {
            p.run(block);
        }
        let dt = t0.elapsed().as_secs_f64() * 1e6;
        for (k, p) in plugs.iter_mut().enumerate() {
            p.collect(&mut outs[k], block);
        }
        if blk * block >= warm {
            times.push(dt / copies as f64);
            for i in 0..block {
                for ch in 0..2 {
                    let v = outs[0][ch][i];
                    sum_sq += (v * v) as f64;
                    peak = peak.max(v.abs());
                }
            }
            cnt += block * 2;
        }
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let db = |x: f64| 20.0 * x.max(1e-12).log10();
    Stat { median_us: times[times.len() / 2], p99_us: times[(times.len() as f64 * 0.99) as usize], budget_us: block as f64 / sr * 1e6, out_rms_db: db((sum_sq / cnt as f64).sqrt()), out_peak_db: db(peak as f64) }
}

fn main() {
    set_ftz();
    // like a host's audio thread: the highest scheduling class (macOS QOS_CLASS_USER_INTERACTIVE = 0x21), so background load does not stall blocks
    #[cfg(target_os = "macos")]
    unsafe {
        pthread_set_qos_class_self_np(0x21, 0);
    }
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(|s| s.as_str()) {
        Some("list") => {
            let path = &a[2];
            let mut p: Box<dyn Plug> = if path.ends_with(".clap") { Box::new(Clap::load(path, 44100.0, 512)) } else { Box::new(Vst2::load(path, 44100.0, 512)) };
            println!("{}", p.name());
            for l in p.params() {
                println!("  {l}");
            }
            let _ = &mut p;
        }
        Some("probe") => {
            let mut p = Vst2::load(&a[2], 44100.0, 512);
            let inp = [(0..512).map(|i| 0.5 * (i as f32 * 0.1).sin()).collect::<Vec<f32>>(), (0..512).map(|i| 0.5 * (i as f32 * 0.1).sin()).collect::<Vec<f32>>()];
            let mut out = [vec![0.0f32; 512], vec![0.0f32; 512]];
            for k in 0..6 {
                p.prepare(&inp, 512);
                let t0 = Instant::now();
                p.run(512);
                let dtt = t0.elapsed().as_secs_f64() * 1e6;
                p.collect(&mut out, 512);
                let peak = out[0].iter().fold(0.0f32, |m, v| m.max(v.abs()));
                println!("block {k}: {:.1} us, out peak {peak}", dtt);
            }
        }
        Some("run") => {
            let wav = read_wav(&a[2]);
            let block: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(512);
            let secs: f64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(30.0);
            println!("audio: {} ({:.1} s, {} Hz), block {block}, {secs} s measured after a 3 s warm-up, one thread, flush-to-zero on\n", a[2], wav.0.len() as f64 / wav.2, wav.2);
            println!("| case | copies | median us/block | p99 us/block | CPU % of one core (median, per copy) | p99 % of the block budget | out RMS dBFS | out peak dBFS |");
            println!("|---|---|---|---|---|---|---|---|");
            for c in suite() {
                for &n in &[1usize, 16] {
                    let s = run_case(&c, n, &wav, block, secs);
                    println!("| {} | {} | {:.1} | {:.1} | {:.2} | {:.2} | {:.1} | {:.1} |", c.label, n, s.median_us, s.p99_us, s.median_us / s.budget_us * 100.0, s.p99_us / s.budget_us * 100.0, s.out_rms_db, s.out_peak_db);
                }
            }
        }
        _ => eprintln!("usage: plugbench list <plug-in> | plugbench run <audio.wav> [block] [secs]"),
    }
}
