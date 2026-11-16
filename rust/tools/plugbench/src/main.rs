//! plugbench: an offline host that loads CLAP (SatComp family) and VST2 (Cockos ReaPlugs) plug-ins, pushes real audio through them
//! block by block on ONE thread (like a host's audio thread, flush-to-zero set like REAPER) and times every block.
//!
//!   plugbench list <path>                       parameter names of a plug-in
//!   plugbench probe <path>                      load, run a sine through it, report peak / time (any format)
//!   plugbench check <dir>                       probe every plug-in in a folder, one process each
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

// ---------------------------------------------------------------------------------------------------------------- VST3
//
// A minimal VST3 host written against the raw COM layout (macOS: IIDs are plain big-endian bytes, no COM swizzle, default struct packing).
// Everything the plug-ins call back (IHostApplication, IComponentHandler, IParameterChanges, IParamValueQueue, IEventList) is a leaked,
// single-threaded object with a static vtable.

type Iid = [u8; 16];
type Tr = i32;

const fn iid(a: u32, b: u32, c: u32, d: u32) -> Iid {
    let (a, b, c, d) = (a.to_be_bytes(), b.to_be_bytes(), c.to_be_bytes(), d.to_be_bytes());
    [a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3], c[0], c[1], c[2], c[3], d[0], d[1], d[2], d[3]]
}
const IID_FUNKNOWN: Iid = iid(0, 0, 0xC000_0000, 0x46);
const IID_HOST_APP: Iid = iid(0x58E595CC, 0xDB2D4969, 0x8B6AAF8C, 0x36A664E5);
const IID_HANDLER: Iid = iid(0x93A0BEA3, 0x0BD045DB, 0x8E890B0C, 0xC1E46AC6);
const IID_COMPONENT: Iid = iid(0xE831FF31, 0xF2D54301, 0x928EBBEE, 0x25697802);
const IID_PROCESSOR: Iid = iid(0x42043F99, 0xB7DA453C, 0xA569E79D, 0x9AAEC33D);
const IID_CONTROLLER: Iid = iid(0xDCD7BBE3, 0x7742448D, 0xA874AACC, 0x979C759E);
const IID_PARAM_CHANGES: Iid = iid(0xA4779663, 0x0BB64A56, 0xB44384A8, 0x466FEB9D);
const IID_PARAM_QUEUE: Iid = iid(0x01263A18, 0xED074F6F, 0x98C9D356, 0x4686F9BA);
const IID_CONNECTION: Iid = iid(0x70A4156F, 0x6E6E4026, 0x989148BF, 0xAA60D8D1);
const IID_EVENT_LIST: Iid = iid(0x3A2C4214, 0x346349FE, 0xB2C4F397, 0xB9695A44);

#[repr(C)]
struct Unk {
    qi: unsafe extern "C" fn(*mut c_void, *const Iid, *mut *mut c_void) -> Tr,
    add_ref: unsafe extern "C" fn(*mut c_void) -> u32,
    release: unsafe extern "C" fn(*mut c_void) -> u32,
}
unsafe extern "C" fn add_ref(_: *mut c_void) -> u32 {
    1
}
unsafe extern "C" fn release(_: *mut c_void) -> u32 {
    1
}
/// a query-interface function that answers FUnknown and one interface with the object itself
macro_rules! qi_fn {
    ($name:ident, $iid:expr) => {
        unsafe extern "C" fn $name(this: *mut c_void, i: *const Iid, out: *mut *mut c_void) -> Tr {
            if *i == IID_FUNKNOWN || *i == $iid {
                *out = this;
                0
            } else {
                *out = std::ptr::null_mut();
                -1
            }
        }
    };
}
qi_fn!(qi_host, IID_HOST_APP);
qi_fn!(qi_handler, IID_HANDLER);
qi_fn!(qi_changes, IID_PARAM_CHANGES);
qi_fn!(qi_queue, IID_PARAM_QUEUE);
qi_fn!(qi_events, IID_EVENT_LIST);

// ---- host application + component handler
#[repr(C)]
struct HostVt {
    u: Unk,
    get_name: unsafe extern "C" fn(*mut c_void, *mut u16) -> Tr,
    create_instance: unsafe extern "C" fn(*mut c_void, *const Iid, *const Iid, *mut *mut c_void) -> Tr,
}
unsafe extern "C" fn host_name(_: *mut c_void, name: *mut u16) -> Tr {
    for (i, ch) in "plugbench".encode_utf16().chain(std::iter::once(0)).enumerate() {
        *name.add(i) = ch;
    }
    0
}
unsafe extern "C" fn host_create(_: *mut c_void, _: *const Iid, _: *const Iid, out: *mut *mut c_void) -> Tr {
    *out = std::ptr::null_mut(); // no IMessage / IAttributeList
    -1
}
static HOST_VT: HostVt = HostVt { u: Unk { qi: qi_host, add_ref, release }, get_name: host_name, create_instance: host_create };
#[repr(C)]
struct Obj<T> {
    vt: *const T,
}

#[repr(C)]
struct HandlerVt {
    u: Unk,
    begin_edit: unsafe extern "C" fn(*mut c_void, u32) -> Tr,
    perform_edit: unsafe extern "C" fn(*mut c_void, u32, f64) -> Tr,
    end_edit: unsafe extern "C" fn(*mut c_void, u32) -> Tr,
    restart: unsafe extern "C" fn(*mut c_void, i32) -> Tr,
}
unsafe extern "C" fn h_edit(_: *mut c_void, _: u32) -> Tr {
    0
}
unsafe extern "C" fn h_perform(_: *mut c_void, _: u32, _: f64) -> Tr {
    0
}
unsafe extern "C" fn h_restart(_: *mut c_void, _: i32) -> Tr {
    0
}
static HANDLER_VT: HandlerVt = HandlerVt { u: Unk { qi: qi_handler, add_ref, release }, begin_edit: h_edit, perform_edit: h_perform, end_edit: h_edit, restart: h_restart };

// ---- parameter changes (input: what we set; output: a sink)
#[repr(C)]
struct QueueVt {
    u: Unk,
    id: unsafe extern "C" fn(*mut c_void) -> u32,
    count: unsafe extern "C" fn(*mut c_void) -> i32,
    get_point: unsafe extern "C" fn(*mut c_void, i32, *mut i32, *mut f64) -> Tr,
    add_point: unsafe extern "C" fn(*mut c_void, i32, f64, *mut i32) -> Tr,
}
#[repr(C)]
struct Queue {
    vt: *const QueueVt,
    id: u32,
    pts: Vec<(i32, f64)>,
}
unsafe extern "C" fn q_id(t: *mut c_void) -> u32 {
    (*(t as *mut Queue)).id
}
unsafe extern "C" fn q_count(t: *mut c_void) -> i32 {
    (*(t as *mut Queue)).pts.len() as i32
}
unsafe extern "C" fn q_get(t: *mut c_void, i: i32, off: *mut i32, v: *mut f64) -> Tr {
    match (&(*(t as *mut Queue)).pts).get(i as usize) {
        Some(p) => {
            *off = p.0;
            *v = p.1;
            0
        }
        None => 1,
    }
}
unsafe extern "C" fn q_add(t: *mut c_void, off: i32, v: f64, idx: *mut i32) -> Tr {
    let q = &mut *(t as *mut Queue);
    q.pts.push((off, v));
    if !idx.is_null() {
        *idx = q.pts.len() as i32 - 1;
    }
    0
}
static QUEUE_VT: QueueVt = QueueVt { u: Unk { qi: qi_queue, add_ref, release }, id: q_id, count: q_count, get_point: q_get, add_point: q_add };

#[repr(C)]
struct ChangesVt {
    u: Unk,
    count: unsafe extern "C" fn(*mut c_void) -> i32,
    get: unsafe extern "C" fn(*mut c_void, i32) -> *mut c_void,
    add: unsafe extern "C" fn(*mut c_void, *const u32, *mut i32) -> *mut c_void,
}
#[repr(C)]
struct Changes {
    vt: *const ChangesVt,
    items: Vec<*mut Queue>,
}
impl Changes {
    fn new() -> Box<Changes> {
        Box::new(Changes { vt: &CHANGES_VT, items: Vec::new() })
    }
    fn clear(&mut self) {
        for q in self.items.drain(..) {
            drop(unsafe { Box::from_raw(q) });
        }
    }
    fn push(&mut self, id: u32, value: f64) {
        let q = Box::into_raw(Box::new(Queue { vt: &QUEUE_VT, id, pts: vec![(0, value)] }));
        self.items.push(q);
    }
}
unsafe extern "C" fn c_count(t: *mut c_void) -> i32 {
    (*(t as *mut Changes)).items.len() as i32
}
unsafe extern "C" fn c_get(t: *mut c_void, i: i32) -> *mut c_void {
    (&(*(t as *mut Changes)).items).get(i as usize).map_or(std::ptr::null_mut(), |q| *q as *mut c_void)
}
unsafe extern "C" fn c_add(t: *mut c_void, id: *const u32, idx: *mut i32) -> *mut c_void {
    let c = &mut *(t as *mut Changes);
    c.push(*id, 0.0);
    c.items.last_mut().unwrap().as_mut().unwrap().pts.clear();
    if !idx.is_null() {
        *idx = c.items.len() as i32 - 1;
    }
    *c.items.last().unwrap() as *mut c_void
}
static CHANGES_VT: ChangesVt = ChangesVt { u: Unk { qi: qi_changes, add_ref, release }, count: c_count, get: c_get, add: c_add };

// ---- empty event list
#[repr(C)]
struct EventsVt {
    u: Unk,
    count: unsafe extern "C" fn(*mut c_void) -> i32,
    get: unsafe extern "C" fn(*mut c_void, i32, *mut c_void) -> Tr,
    add: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Tr,
}
unsafe extern "C" fn e_count(_: *mut c_void) -> i32 {
    0
}
unsafe extern "C" fn e_get(_: *mut c_void, _: i32, _: *mut c_void) -> Tr {
    1
}
unsafe extern "C" fn e_add(_: *mut c_void, _: *mut c_void) -> Tr {
    0
}
static EVENTS_VT: EventsVt = EventsVt { u: Unk { qi: qi_events, add_ref, release }, count: e_count, get: e_get, add: e_add };

// ---- the plug-in's interfaces
#[repr(C)]
struct FactoryVt {
    u: Unk,
    factory_info: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Tr,
    count: unsafe extern "C" fn(*mut c_void) -> i32,
    class_info: unsafe extern "C" fn(*mut c_void, i32, *mut ClassInfo) -> Tr,
    create: unsafe extern "C" fn(*mut c_void, *const Iid, *const Iid, *mut *mut c_void) -> Tr,
}
#[repr(C)]
struct ClassInfo {
    cid: Iid,
    cardinality: i32,
    category: [u8; 32],
    name: [u8; 64],
}
#[repr(C)]
struct CompVt {
    u: Unk,
    initialize: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Tr,
    terminate: unsafe extern "C" fn(*mut c_void) -> Tr,
    controller_class: unsafe extern "C" fn(*mut c_void, *mut Iid) -> Tr,
    set_io_mode: unsafe extern "C" fn(*mut c_void, i32) -> Tr,
    bus_count: unsafe extern "C" fn(*mut c_void, i32, i32) -> i32,
    bus_info: unsafe extern "C" fn(*mut c_void, i32, i32, i32, *mut BusInfo) -> Tr,
    routing: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> Tr,
    activate_bus: unsafe extern "C" fn(*mut c_void, i32, i32, i32, u8) -> Tr,
    set_active: unsafe extern "C" fn(*mut c_void, u8) -> Tr,
    set_state: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Tr,
    get_state: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Tr,
}
#[repr(C)]
struct BusInfo {
    media: i32,
    dir: i32,
    channels: i32,
    name: [u16; 128],
    bus_type: i32,
    flags: u32,
}
#[repr(C)]
struct ProcVt {
    u: Unk,
    set_bus_arr: unsafe extern "C" fn(*mut c_void, *mut u64, i32, *mut u64, i32) -> Tr,
    get_bus_arr: unsafe extern "C" fn(*mut c_void, i32, i32, *mut u64) -> Tr,
    can_process: unsafe extern "C" fn(*mut c_void, i32) -> Tr,
    latency: unsafe extern "C" fn(*mut c_void) -> u32,
    setup: unsafe extern "C" fn(*mut c_void, *const ProcessSetup) -> Tr,
    set_processing: unsafe extern "C" fn(*mut c_void, u8) -> Tr,
    process: unsafe extern "C" fn(*mut c_void, *mut ProcessData) -> Tr,
    tail: unsafe extern "C" fn(*mut c_void) -> u32,
}
#[repr(C)]
struct ProcessSetup {
    mode: i32,
    sample_size: i32,
    max_block: i32,
    sample_rate: f64,
}
#[repr(C)]
struct BusBuffers {
    channels: i32,
    silence: u64,
    buffers: *mut *mut f32,
}
#[repr(C)]
struct ProcessData {
    mode: i32,
    sample_size: i32,
    num_samples: i32,
    num_inputs: i32,
    num_outputs: i32,
    inputs: *mut BusBuffers,
    outputs: *mut BusBuffers,
    in_changes: *mut c_void,
    out_changes: *mut c_void,
    in_events: *mut c_void,
    out_events: *mut c_void,
    context: *mut c_void,
}
#[repr(C)]
struct CtrlVt {
    u: Unk,
    initialize: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Tr,
    terminate: unsafe extern "C" fn(*mut c_void) -> Tr,
    set_component_state: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Tr,
    set_state: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Tr,
    get_state: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Tr,
    param_count: unsafe extern "C" fn(*mut c_void) -> i32,
    param_info: unsafe extern "C" fn(*mut c_void, i32, *mut ParamInfo) -> Tr,
    string_by_value: unsafe extern "C" fn(*mut c_void, u32, f64, *mut u16) -> Tr,
    value_by_string: unsafe extern "C" fn(*mut c_void, u32, *const u16, *mut f64) -> Tr,
    norm_to_plain: unsafe extern "C" fn(*mut c_void, u32, f64) -> f64,
    plain_to_norm: unsafe extern "C" fn(*mut c_void, u32, f64) -> f64,
    get_norm: unsafe extern "C" fn(*mut c_void, u32) -> f64,
    set_norm: unsafe extern "C" fn(*mut c_void, u32, f64) -> Tr,
    set_handler: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Tr,
    create_view: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
}
#[repr(C)]
struct ConnVt {
    u: Unk,
    connect: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Tr,
    disconnect: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Tr,
    notify: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Tr,
}
#[repr(C)]
struct ParamInfo {
    id: u32,
    title: [u16; 128],
    short_title: [u16; 128],
    units: [u16; 128],
    steps: i32,
    default: f64,
    unit_id: i32,
    flags: i32,
}

unsafe fn vt<T>(o: *mut c_void) -> &'static T {
    &**(o as *mut *const T)
}
fn u16s(s: &[u16]) -> String {
    String::from_utf16_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())])
}
fn cstr(s: &[u8]) -> String {
    String::from_utf8_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())]).to_string()
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFURLCreateFromFileSystemRepresentation(alloc: *const c_void, buf: *const u8, len: isize, is_dir: u8) -> *const c_void;
    fn CFBundleCreate(alloc: *const c_void, url: *const c_void) -> *const c_void;
}

struct Vst3 {
    ctrl: *mut c_void,
    proc_: *mut c_void,
    name: String,
    params: Vec<(String, u32)>,
    in_buses: i32,
    out_buses: i32,
    in_ch: usize,
    out_ch: usize,
    latency: u32,
    bin: Vec<Vec<f32>>,
    bout: Vec<Vec<f32>>,
    changes_in: Box<Changes>,
    changes_out: Box<Changes>,
    events: Box<Obj<EventsVt>>,
    warned: bool,
    aux_ch: Vec<usize>,
    aux_zero: Vec<f32>,
}

impl Vst3 {
    fn load(bundle: &str, sr: f64, max_block: i32) -> Self {
        let stem = std::path::Path::new(bundle).file_stem().unwrap().to_string_lossy().to_string();
        let lib = open_lib(&format!("{bundle}/Contents/MacOS/{stem}"));
        unsafe {
            // macOS modules get their CFBundle through bundleEntry
            let entry = sym(lib, "bundleEntry");
            #[cfg(target_os = "macos")]
            if !entry.is_null() {
                let url = CFURLCreateFromFileSystemRepresentation(std::ptr::null(), bundle.as_ptr(), bundle.len() as isize, 1);
                let bun = CFBundleCreate(std::ptr::null(), url);
                let f: extern "C" fn(*const c_void) -> bool = std::mem::transmute(entry);
                assert!(f(bun), "bundleEntry failed in {bundle}");
            }
            let _ = entry;
            let gf = sym(lib, "GetPluginFactory");
            assert!(!gf.is_null(), "no GetPluginFactory in {bundle}");
            let gf: extern "C" fn() -> *mut c_void = std::mem::transmute(gf);
            let fac = gf();
            assert!(!fac.is_null(), "GetPluginFactory returned null");
            let host = Box::leak(Box::new(Obj { vt: &HOST_VT as *const HostVt })) as *mut Obj<HostVt> as *mut c_void;
            let handler = Box::leak(Box::new(Obj { vt: &HANDLER_VT as *const HandlerVt })) as *mut Obj<HandlerVt> as *mut c_void;
            let fv = vt::<FactoryVt>(fac);
            let mut cid = None;
            let mut name = String::new();
            for i in 0..(fv.count)(fac) {
                let mut ci: ClassInfo = std::mem::zeroed();
                if (fv.class_info)(fac, i, &mut ci) == 0 && cstr(&ci.category) == "Audio Module Class" {
                    cid = Some(ci.cid);
                    name = cstr(&ci.name);
                    break;
                }
            }
            let cid = cid.unwrap_or_else(|| panic!("no audio class in {bundle}"));
            let mut comp: *mut c_void = std::ptr::null_mut();
            assert!((fv.create)(fac, &cid, &IID_COMPONENT, &mut comp) == 0 && !comp.is_null(), "cannot create the component of {bundle}");
            let cv = vt::<CompVt>(comp);
            let r = (cv.initialize)(comp, host);
            assert!(r == 0, "component initialize failed ({r})");
            let mut proc_: *mut c_void = std::ptr::null_mut();
            assert!((cv.u.qi)(comp, &IID_PROCESSOR, &mut proc_) == 0 && !proc_.is_null(), "no IAudioProcessor");
            // the controller: the component itself, or a separate class
            let mut ctrl: *mut c_void = std::ptr::null_mut();
            if (cv.u.qi)(comp, &IID_CONTROLLER, &mut ctrl) != 0 || ctrl.is_null() {
                let mut ccid: Iid = [0; 16];
                ctrl = std::ptr::null_mut();
                let r1 = (cv.controller_class)(comp, &mut ccid);
                let r2 = if r1 == 0 { (fv.create)(fac, &ccid, &IID_CONTROLLER, &mut ctrl) } else { -99 };
                if std::env::var_os("PLUGBENCH_INFO").is_some() {
                    eprintln!("  vst3: getControllerClassId {r1}, create controller {r2}");
                }
                if r1 == 0 && r2 == 0 && !ctrl.is_null() {
                    let r = (vt::<CtrlVt>(ctrl).initialize)(ctrl, host);
                    assert!(r == 0, "controller initialize failed ({r})");
                }
            }
            // split component / controller (JUCE): link the two through IConnectionPoint, the controller learns its parameters that way
            if !ctrl.is_null() && ctrl != comp {
                let (mut cp_comp, mut cp_ctrl): (*mut c_void, *mut c_void) = (std::ptr::null_mut(), std::ptr::null_mut());
                let a = (cv.u.qi)(comp, &IID_CONNECTION, &mut cp_comp);
                let b = (vt::<CtrlVt>(ctrl).u.qi)(ctrl, &IID_CONNECTION, &mut cp_ctrl);
                if std::env::var_os("PLUGBENCH_INFO").is_some() {
                    eprintln!("  vst3: IConnectionPoint on component {a}, on controller {b}; ctrl {:p} comp {:p}", ctrl, comp);
                    let unit = iid(0x3D4BD6B5, 0x913A4FD2, 0xA886E768, 0xA5EB92C1);
                    for (nm, id) in [("IUnitInfo", unit), ("IEditController", IID_CONTROLLER), ("IComponent", IID_COMPONENT), ("IAudioProcessor", IID_PROCESSOR)] {
                        let (mut x, mut y): (*mut c_void, *mut c_void) = (std::ptr::null_mut(), std::ptr::null_mut());
                        let ra = (cv.u.qi)(comp, &id, &mut x);
                        let rb = (vt::<CtrlVt>(ctrl).u.qi)(ctrl, &id, &mut y);
                        eprintln!("    qi {nm}: component {ra}, controller {rb}");
                    }
                }
                if a == 0 && b == 0 && !cp_comp.is_null() && !cp_ctrl.is_null() {
                    let ra = (vt::<ConnVt>(cp_comp).connect)(cp_comp, cp_ctrl);
                    let rb = (vt::<ConnVt>(cp_ctrl).connect)(cp_ctrl, cp_comp);
                    if std::env::var_os("PLUGBENCH_INFO").is_some() {
                        eprintln!("  vst3: connected component <-> controller ({ra}, {rb})");
                    }
                }
            }
            let mut params = Vec::new();
            if std::env::var_os("PLUGBENCH_INFO").is_some() {
                eprintln!("  vst3: controller {}", if ctrl.is_null() { "MISSING" } else { "found" });
            }
            if !ctrl.is_null() {
                let kv = vt::<CtrlVt>(ctrl);
                (kv.set_handler)(ctrl, handler);
                if std::env::var_os("PLUGBENCH_INFO").is_some() {
                    eprintln!("  vst3: controller reports {} parameters", (kv.param_count)(ctrl));
                }
                for i in 0..(kv.param_count)(ctrl) {
                    let mut pi: ParamInfo = std::mem::zeroed();
                    if (kv.param_info)(ctrl, i, &mut pi) == 0 {
                        params.push((u16s(&pi.title), pi.id));
                    }
                }
            }
            // buses: the main bus of each direction active with 2 channels (or what the plug-in insists on); aux buses stay off like in a host
            let pv = vt::<ProcVt>(proc_);
            let (n_in, n_out) = ((cv.bus_count)(comp, 0, 0), (cv.bus_count)(comp, 0, 1));
            let arr = |dir: i32, n: i32| -> Vec<u64> {
                (0..n)
                    .map(|b| {
                        let mut a = 3u64; // kStereo
                        if b > 0 {
                            a = 0;
                            (pv.get_bus_arr)(proc_, dir, b, &mut a);
                        }
                        a
                    })
                    .collect()
            };
            let (mut ai, mut ao) = (arr(0, n_in), arr(1, n_out));
            (pv.set_bus_arr)(proc_, ai.as_mut_ptr(), n_in, ao.as_mut_ptr(), n_out);
            let chans = |dir: i32| -> usize {
                let mut bi: BusInfo = std::mem::zeroed();
                if (cv.bus_info)(comp, 0, dir, 0, &mut bi) == 0 {
                    bi.channels.max(0) as usize
                } else {
                    0
                }
            };
            let (in_ch, out_ch) = (chans(0), chans(1));
            if n_in > 0 {
                (cv.activate_bus)(comp, 0, 0, 0, 1);
            }
            if n_out > 0 {
                (cv.activate_bus)(comp, 0, 1, 0, 1);
            }
            let mut aux_ch = vec![0usize; n_in.max(0) as usize];
            // a host that offers a sidechain bus has it active; some plug-ins (TDR Molotok) pass audio through untouched when it is off
            if std::env::var_os("PLUGBENCH_NOAUX").is_none() {
                for b in 1..n_in {
                    let mut bi: BusInfo = std::mem::zeroed();
                    if (cv.bus_info)(comp, 0, 0, b, &mut bi) == 0 {
                        aux_ch[b as usize] = bi.channels.max(0) as usize;
                        (cv.activate_bus)(comp, 0, 0, b, 1);
                    }
                }
            }
            assert!((pv.can_process)(proc_, 0) == 0, "{bundle} cannot process 32-bit floats");
            let setup = ProcessSetup { mode: 0, sample_size: 0, max_block, sample_rate: sr };
            let r = (pv.setup)(proc_, &setup);
            assert!(r == 0, "setupProcessing failed ({r})");
            (cv.set_active)(comp, 1);
            (pv.set_processing)(proc_, 1);
            let mb = max_block as usize;
            Vst3 {
                ctrl,
                proc_,
                name,
                params,
                in_buses: n_in,
                out_buses: n_out,
                in_ch,
                out_ch,
                latency: (pv.latency)(proc_),
                bin: (0..in_ch.max(1)).map(|_| vec![0.0; mb]).collect(),
                bout: (0..out_ch.max(1)).map(|_| vec![0.0; mb]).collect(),
                changes_in: Changes::new(),
                changes_out: Changes::new(),
                events: Box::new(Obj { vt: &EVENTS_VT }),
                warned: false,
                aux_ch,
                aux_zero: vec![0.0; mb],
            }
        }
    }
    fn text(&self, id: u32, v: f64) -> String {
        let mut buf = [0u16; 128];
        unsafe { (vt::<CtrlVt>(self.ctrl).string_by_value)(self.ctrl, id, v, buf.as_mut_ptr()) };
        u16s(&buf)
    }
    fn find(&self, name: &str) -> Option<usize> {
        if let Some(n) = name.strip_prefix('#') {
            return n.parse().ok().filter(|&i| i < self.params.len());
        }
        let l = name.to_lowercase();
        self.params.iter().position(|p| p.0.to_lowercase() == l).or_else(|| self.params.iter().position(|p| p.0.to_lowercase().contains(&l)))
    }
    fn write(&mut self, idx: usize, v: f64) {
        let id = self.params[idx].1;
        unsafe { (vt::<CtrlVt>(self.ctrl).set_norm)(self.ctrl, id, v) };
        self.changes_in.push(id, v);
    }
}

impl Plug for Vst3 {
    fn name(&self) -> String {
        self.name.clone()
    }
    fn params(&self) -> Vec<String> {
        let mut v = vec![format!("buses: {} in / {} out, main bus {} in / {} out channels, latency {} samples", self.in_buses, self.out_buses, self.in_ch, self.out_ch, self.latency)];
        if self.ctrl.is_null() {
            return v;
        }
        for (i, (n, id)) in self.params.iter().enumerate() {
            let cur = unsafe { (vt::<CtrlVt>(self.ctrl).get_norm)(self.ctrl, *id) };
            v.push(format!("#{i} {n} = {} (norm {:.3})", self.text(*id, cur), cur));
        }
        v
    }
    fn set(&mut self, name: &str, plain: f64, min: f64, max: f64) -> bool {
        let Some(i) = self.find(name) else { return false };
        self.write(i, ((plain - min) / (max - min)).clamp(0.0, 1.0));
        true
    }
    fn set_display(&mut self, name: &str, target: f64) -> bool {
        let Some(i) = self.find(name) else { return false };
        let id = self.params[i].1;
        let parse = |s: &str| s.split_whitespace().next().and_then(|t| t.trim_end_matches(|c: char| !c.is_ascii_digit()).parse::<f64>().ok());
        let (Some(a), Some(b)) = (parse(&self.text(id, 0.02)), parse(&self.text(id, 0.98))) else { return false };
        let up = b > a;
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            let Some(d) = parse(&self.text(id, mid)) else { return false };
            if (d < target) == up {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        self.write(i, 0.5 * (lo + hi));
        true
    }
    fn prepare(&mut self, inp: &[Vec<f32>; 2], n: usize) {
        for ch in 0..self.bin.len() {
            self.bin[ch][..n].copy_from_slice(&inp[ch.min(1)][..n]);
        }
    }
    fn collect(&mut self, out: &mut [Vec<f32>; 2], n: usize) {
        for ch in 0..2 {
            out[ch][..n].copy_from_slice(&self.bout[ch.min(self.bout.len() - 1)][..n]);
        }
    }
    fn run(&mut self, n: usize) {
        unsafe {
            let mut ip: Vec<*mut f32> = self.bin.iter_mut().map(|b| b.as_mut_ptr()).collect();
            let mut op: Vec<*mut f32> = self.bout.iter_mut().map(|b| b.as_mut_ptr()).collect();
            let mk = |n_buses: i32, ch: usize, p: &mut Vec<*mut f32>| -> Vec<BusBuffers> {
                (0..n_buses).map(|b| if b == 0 { BusBuffers { channels: ch as i32, silence: 0, buffers: p.as_mut_ptr() } } else { BusBuffers { channels: 0, silence: 0, buffers: std::ptr::null_mut() } }).collect()
            };
            // an activated sidechain bus gets silence (flagged as such)
            let mut aux_ptrs: Vec<Vec<*mut f32>> = self.aux_ch.iter().map(|&c| vec![self.aux_zero.as_ptr() as *mut f32; c]).collect();
            let mut ins = mk(self.in_buses, self.in_ch, &mut ip);
            for (b, bus) in ins.iter_mut().enumerate().skip(1) {
                if self.aux_ch[b] > 0 {
                    *bus = BusBuffers { channels: self.aux_ch[b] as i32, silence: !0, buffers: aux_ptrs[b].as_mut_ptr() };
                }
            }
            let mut outs = mk(self.out_buses, self.out_ch, &mut op);
            let ev = &mut *self.events as *mut Obj<EventsVt> as *mut c_void;
            let mut pd = ProcessData {
                mode: 0,
                sample_size: 0,
                num_samples: n as i32,
                num_inputs: ins.len() as i32,
                num_outputs: outs.len() as i32,
                inputs: ins.as_mut_ptr(),
                outputs: outs.as_mut_ptr(),
                in_changes: &mut *self.changes_in as *mut Changes as *mut c_void,
                out_changes: &mut *self.changes_out as *mut Changes as *mut c_void,
                in_events: ev,
                out_events: ev,
                context: std::ptr::null_mut(),
            };
            let r = (vt::<ProcVt>(self.proc_).process)(self.proc_, &mut pd);
            if r != 0 && !self.warned {
                self.warned = true;
                eprintln!("  vst3: process() returned {r}");
            }
            self.changes_in.clear(); // parameter writes are delivered once
            self.changes_out.clear();
        }
    }
    fn latency(&self) -> u32 {
        self.latency
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
    Vst3,
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
fn vst3(label: &'static str, path: &'static str, set: Set) -> Case {
    Case { label, kind: Kind::Vst3, path, set }
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
    let sd = "../../target/bundled/satdiff.clap";
    let kot = "../../../vst/TDR Kotelnikov.vst3";
    let mol = "../../../vst/TDR Molotok.vst3";
    let tl6 = "../../../vst/TDR Limiter 6 GE.vst3";
    let fa = "../../../vst/FreeAMP.vst3";
    let sz = "../../../vst/Szechuan.vst3";
    vec![
        // ---- single-band compressors
        vst("ReaComp (defaults: no reduction)", rc, vec![]),
        vst("ReaComp (-20 dB, 4:1, 10 / 150 ms)", rc, vec![d(0, -20.0), d(1, 4.0), d(2, 10.0), d(3, 150.0)]),
        clap("SatComp, compressor only (limiter off)", sc, cat(vec![sc_comp(), vec![c("Limiter", 0.0, 0.0, 1.0)]])),
        clap("SatComp, as shipped (limiter on)", sc, sc_comp()),
        clap("SatComp + saturator drive 30 %, x1", sc, cat(vec![sc_comp(), vec![c("Drive", 30.0, 0.0, 100.0)]])),
        clap("SatComp + saturator drive 30 %, x4 oversampling", sc, cat(vec![sc_comp(), vec![c("Drive", 30.0, 0.0, 100.0), c("Oversampling", 2.0, 0.0, 2.0)]])),
        // VST3 parameters are set by the index of `plugbench list` so that the DISPLAY text reads the value (dB, ms, :1)
        vst3("TDR Kotelnikov (defaults)", kot, vec![]),
        vst3("TDR Kotelnikov (-20 dB, 4:1, 10 / 150 ms)", kot, vec![d(0, -20.0), d(3, 4.0), d(4, 10.0), d(5, 150.0)]),
        vst3("TDR Molotok (defaults)", mol, vec![]),
        vst3("TDR Molotok (-20 dB, 4:1, 10 / 150 ms)", mol, vec![d(6, -20.0), d(5, 4.0), d(2, 10.0), d(4, 150.0)]),
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
        vst3("TDR Limiter 6 GE (defaults)", tl6, vec![]),
        vst3("TDR Limiter 6 GE (peak limiter +6 dB into the ceiling)", tl6, vec![d(18, 6.0)]),
        // ---- saturators
        clap("SatDiff (defaults)", sd, vec![]),
        clap("SatDiff, drive 50 %, x1", sd, vec![c("Drive", 50.0, 0.0, 100.0)]),
        clap("SatDiff, drive 50 %, x4 oversampling", sd, vec![c("Drive", 50.0, 0.0, 100.0), c("Oversampling", 2.0, 0.0, 3.0)]),
        vst3("FreeAMP (defaults)", fa, vec![]),
        vst3("FreeAMP (drive 80 %)", fa, vec![c("Drive", 0.8, 0.0, 1.0)]),
        vst3("Szechuan Saturator (defaults)", sz, vec![]),
        vst3("Szechuan Saturator (intensity 8)", sz, vec![d(0, 8.0)]),
    ]
}

fn load_any(path: &str, sr: f64, block: i32) -> Box<dyn Plug> {
    if path.ends_with(".clap") {
        Box::new(Clap::load(path, sr, block as u32))
    } else if path.ends_with(".vst3") {
        Box::new(Vst3::load(path, sr, block))
    } else {
        Box::new(Vst2::load(path, sr, block))
    }
}

fn make(c: &Case, sr: f64, block: u32) -> Box<dyn Plug> {
    let mut p: Box<dyn Plug> = match c.kind {
        Kind::Clap => Box::new(Clap::load(c.path, sr, block)),
        Kind::Vst2 => Box::new(Vst2::load(c.path, sr, block as i32)),
        Kind::Vst3 => Box::new(Vst3::load(c.path, sr, block as i32)),
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
            let mut p = load_any(&a[2], 44100.0, 512);
            println!("{}", p.name());
            for l in p.params() {
                println!("  {l}");
            }
            let _ = &mut p;
        }
        // load a plug-in, run a -12 dB sine through it for a second and report whether it came out alive (used by `check`)
        Some("probe") => {
            let mut p = load_any(&a[2], 44100.0, 512);
            let amp: f32 = std::env::var("PLUGBENCH_AMP").ok().and_then(|v| v.parse().ok()).unwrap_or(0.25);
            let inp = [(0..512).map(|i| 0.25 * (i as f32 * 0.1).sin()).collect::<Vec<f32>>(), (0..512).map(|i| 0.25 * (i as f32 * 0.1).sin()).collect::<Vec<f32>>()];
            let mut out = [vec![0.0f32; 512], vec![0.0f32; 512]];
            let mut times = Vec::new();
            let (mut peak, mut finite) = (0.0f32, true);
            for k in 0..90 {
                let mut inb = inp.clone();
                for ch in 0..2 {
                    for i in 0..512 {
                        inb[ch][i] = amp * (((k * 512 + i) as f32) * 0.1).sin();
                    }
                }
                p.prepare(&inb, 512);
                let t0 = Instant::now();
                p.run(512);
                times.push(t0.elapsed().as_secs_f64() * 1e6);
                p.collect(&mut out, 512);
                if k >= 60 {
                    for v in out[0].iter().chain(out[1].iter()) {
                        finite &= v.is_finite();
                        peak = peak.max(v.abs());
                    }
                }
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            println!("{} | latency {} | median {:.1} us/block | in peak {:.1} dBFS, out peak {:.1} dBFS{}", p.name(), p.latency(), times[times.len() / 2], 20.0 * (amp as f64).log10(), 20.0 * (peak as f64).max(1e-9).log10(), if finite { "" } else { " | NON-FINITE OUTPUT" });
        }
        // every VST3 / VST2 / CLAP in a folder, each probed in its own process so a crash only fails that one
        Some("check") => {
            let mut paths: Vec<_> = std::fs::read_dir(&a[2]).expect("dir").filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("vst3" | "clap" | "dylib"))).collect();
            paths.sort();
            let exe = std::env::current_exe().unwrap();
            let mut bad = 0;
            for p in paths {
                let o = std::process::Command::new(&exe).arg("probe").arg(&p).output().expect("spawn");
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                if o.status.success() {
                    println!("OK    {name}: {}", String::from_utf8_lossy(&o.stdout).trim());
                } else {
                    bad += 1;
                    let err = String::from_utf8_lossy(&o.stderr);
                    let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
                    println!("FAIL  {name}: {} {}", o.status, last);
                }
            }
            println!("\n{bad} failed");
        }
        Some("run") => {
            let wav = read_wav(&a[2]);
            let block: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(512);
            let secs: f64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(30.0);
            println!("audio: {} ({:.1} s, {} Hz), block {block}, {secs} s measured after a 3 s warm-up, one thread, flush-to-zero on\n", a[2], wav.0.len() as f64 / wav.2, wav.2);
            println!("| case | copies | median us/block | p99 us/block | CPU % of one core (median, per copy) | p99 % of the block budget | out RMS dBFS | out peak dBFS |");
            println!("|---|---|---|---|---|---|---|---|");
            // PLUGBENCH_CASE=text: only the cases whose label contains it (to repeat one measurement)
            let only = std::env::var("PLUGBENCH_CASE").ok();
            for c in suite() {
                if only.as_ref().map_or(false, |o| !c.label.contains(o.as_str())) {
                    continue;
                }
                for &n in &[1usize, 16] {
                    let s = run_case(&c, n, &wav, block, secs);
                    println!("| {} | {} | {:.1} | {:.1} | {:.2} | {:.2} | {:.1} | {:.1} |", c.label, n, s.median_us, s.p99_us, s.median_us / s.budget_us * 100.0, s.p99_us / s.budget_us * 100.0, s.out_rms_db, s.out_peak_db);
                }
            }
        }
        _ => eprintln!("usage: plugbench list <plug-in> | plugbench probe <plug-in> | plugbench check <dir> | plugbench run <audio.wav> [block] [secs]"),
    }
}
