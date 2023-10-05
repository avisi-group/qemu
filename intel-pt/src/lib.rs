use {
    crate::mode::{get_mode, set_mode, Mode},
    once_cell::sync::Lazy,
    parking_lot::Mutex,
    std::{
        ffi::{c_char, CStr},
        fs::File,
        io::{BufWriter, Write},
    },
};

mod intel_pt;
mod mode;

#[no_mangle]
pub extern "C" fn guest_pc_disable_direct_chaining() -> bool {
    get_mode() == Mode::Simple
}

#[no_mangle]
pub extern "C" fn insert_jmx_at_block_start() -> bool {
    get_mode() == Mode::IntelPt
}

#[no_mangle]
pub extern "C" fn trace_guest_pc(pc: u64) {
    static FILE: Lazy<Mutex<BufWriter<File>>> = Lazy::new(|| {
        let f = File::create("/tmp/ipt/simple.trace").unwrap();
        Mutex::new(BufWriter::with_capacity(65536, f))
    });

    if get_mode() == Mode::Simple {
        write!(FILE.lock(), "{pc:X}\n").unwrap();
    }
}

#[no_mangle]
pub extern "C" fn handle_arg_intel_pt(arg: *const c_char) {
    let arg = unsafe { CStr::from_ptr(arg) }.to_str().unwrap();
    match arg {
        // write guest PC directly to file
        "simple" => set_mode(mode::Mode::Simple),
        //
        "intelpt" => {
            set_mode(mode::Mode::IntelPt);
            intel_pt::init();
        }
        "ptwrite" => {
            set_mode(mode::Mode::PtWrite);
            unimplemented!()
        }
        _ => {
            panic!("unrecognized intel pt argument {:?}", arg);
        }
    }
}
