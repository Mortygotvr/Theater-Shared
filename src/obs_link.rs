//! The few libobs functions the relay needs (procedures and signals on OBS's
//! global handlers), found at run time like Theater Reader finds them, so a
//! plugin needs no import library.

use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::OnceLock;

/// libobs's calldata_t.
#[repr(C)]
pub struct Calldata {
    stack: *mut u8,
    size: usize,
    capacity: usize,
    fixed: bool,
}

impl Calldata {
    pub fn new() -> Self {
        Calldata { stack: std::ptr::null_mut(), size: 0, capacity: 0, fixed: false }
    }
}

impl Default for Calldata {
    fn default() -> Self {
        Self::new()
    }
}

pub type ProcFn = unsafe extern "C" fn(data: *mut c_void, cd: *mut Calldata);
pub type SignalFn = unsafe extern "C" fn(data: *mut c_void, cd: *mut Calldata);

pub struct Obs {
    proc_handler_add: unsafe extern "C" fn(*mut c_void, *const c_char, ProcFn, *mut c_void),
    proc_handler_call: unsafe extern "C" fn(*mut c_void, *const c_char, *mut Calldata) -> bool,
    signal_handler_add: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    signal_handler_signal: unsafe extern "C" fn(*mut c_void, *const c_char, *mut Calldata),
    signal_handler_connect: unsafe extern "C" fn(*mut c_void, *const c_char, SignalFn, *mut c_void),
    calldata_get_string: unsafe extern "C" fn(*const Calldata, *const c_char, *mut *const c_char) -> bool,
    calldata_set_data: unsafe extern "C" fn(*mut Calldata, *const c_char, *const c_void, usize),
    bfree: unsafe extern "C" fn(*mut c_void),
    proc_handler: *mut c_void,
    signal_handler: *mut c_void,
}

// OBS's global handlers live as long as OBS, and are thread-safe.
unsafe impl Send for Obs {}
unsafe impl Sync for Obs {}

#[cfg(target_os = "windows")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
}

#[cfg(not(target_os = "windows"))]
unsafe extern "C" {
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}

fn symbol(name: &CStr) -> *mut c_void {
    #[cfg(target_os = "windows")]
    {
        let lib: Vec<u16> = "obs.dll".encode_utf16().chain(std::iter::once(0)).collect();
        let module = unsafe { GetModuleHandleW(lib.as_ptr()) };
        if module.is_null() {
            return std::ptr::null_mut();
        }
        unsafe { GetProcAddress(module, name.as_ptr()) }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // RTLD_DEFAULT: what the process (OBS) has loaded
        unsafe { dlsym(std::ptr::null_mut(), name.as_ptr()) }
    }
}

fn function<T>(name: &CStr) -> Option<T> {
    let p = symbol(name);
    // SAFETY: T is the function pointer type of the export `name`.
    (!p.is_null()).then(|| unsafe { std::mem::transmute_copy(&p) })
}

fn load() -> Option<Obs> {
    let get_proc_handler: unsafe extern "C" fn() -> *mut c_void = function(c"obs_get_proc_handler")?;
    let get_signal_handler: unsafe extern "C" fn() -> *mut c_void = function(c"obs_get_signal_handler")?;
    let proc_handler = unsafe { get_proc_handler() };
    let signal_handler = unsafe { get_signal_handler() };
    if proc_handler.is_null() || signal_handler.is_null() {
        return None;
    }
    Some(Obs {
        proc_handler_add: function(c"proc_handler_add")?,
        proc_handler_call: function(c"proc_handler_call")?,
        signal_handler_add: function(c"signal_handler_add")?,
        signal_handler_signal: function(c"signal_handler_signal")?,
        signal_handler_connect: function(c"signal_handler_connect")?,
        calldata_get_string: function(c"calldata_get_string")?,
        calldata_set_data: function(c"calldata_set_data")?,
        bfree: function(c"bfree")?,
        proc_handler,
        signal_handler,
    })
}

/// libobs, when this runs inside OBS
pub fn obs() -> Option<&'static Obs> {
    static OBS: OnceLock<Option<Obs>> = OnceLock::new();
    OBS.get_or_init(load).as_ref()
}

impl Obs {
    pub fn add_proc(&self, decl: &CStr, f: ProcFn) {
        unsafe { (self.proc_handler_add)(self.proc_handler, decl.as_ptr(), f, std::ptr::null_mut()) }
    }

    /// Calls a procedure; false when there's none by that name
    pub fn call(&self, name: &CStr, cd: &mut Calldata) -> bool {
        unsafe { (self.proc_handler_call)(self.proc_handler, name.as_ptr(), cd) }
    }

    pub fn add_signal(&self, decl: &CStr) {
        unsafe {
            (self.signal_handler_add)(self.signal_handler, decl.as_ptr());
        }
    }

    pub fn connect(&self, name: &CStr, f: SignalFn) {
        unsafe { (self.signal_handler_connect)(self.signal_handler, name.as_ptr(), f, std::ptr::null_mut()) }
    }

    /// Emits a signal with one string, `key`
    pub fn signal_string(&self, name: &CStr, key: &CStr, value: &str) {
        let mut cd = Calldata::new();
        self.set_string(&mut cd, key, value);
        unsafe { (self.signal_handler_signal)(self.signal_handler, name.as_ptr(), &mut cd) };
        self.free(&mut cd);
    }

    pub fn set_string(&self, cd: *mut Calldata, key: &CStr, value: &str) {
        let Ok(value) = CString::new(value.replace('\0', "")) else { return };
        let bytes = value.as_bytes_with_nul();
        unsafe { (self.calldata_set_data)(cd, key.as_ptr(), bytes.as_ptr() as *const c_void, bytes.len()) }
    }

    pub fn set_int(&self, cd: *mut Calldata, key: &CStr, value: i64) {
        unsafe { (self.calldata_set_data)(cd, key.as_ptr(), &value as *const i64 as *const c_void, std::mem::size_of::<i64>()) }
    }

    pub fn set_bool(&self, cd: *mut Calldata, key: &CStr, value: bool) {
        unsafe { (self.calldata_set_data)(cd, key.as_ptr(), &value as *const bool as *const c_void, std::mem::size_of::<bool>()) }
    }

    pub fn get_string(&self, cd: *const Calldata, key: &CStr) -> String {
        let mut s: *const c_char = std::ptr::null();
        if unsafe { (self.calldata_get_string)(cd, key.as_ptr(), &mut s) } && !s.is_null() {
            unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()
        } else {
            String::new()
        }
    }

    /// Frees what libobs allocated for a calldata this side made
    pub fn free(&self, cd: &mut Calldata) {
        if !cd.stack.is_null() {
            unsafe { (self.bfree)(cd.stack as *mut c_void) };
            cd.stack = std::ptr::null_mut();
        }
    }
}
