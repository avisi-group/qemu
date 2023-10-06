use {
    crate::STATE,
    std::ffi::{c_char, CStr},
};

#[no_mangle]
pub extern "C" fn handle_arg_intel_pt(arg: *const c_char) {
    let arg = unsafe { CStr::from_ptr(arg) }.to_str().unwrap();
    match arg {
        "simple" => STATE.init_simple(),
        "intelpt" => STATE.init_intelpt(),
        "ptwrite" => {
            unimplemented!()
        }
        _ => {
            panic!("unrecognized intel pt argument {:?}", arg);
        }
    }
}

#[no_mangle]
pub extern "C" fn intel_pt_enable_direct_chaining() -> bool {
    STATE.enable_direct_chaining()
}

#[no_mangle]
pub extern "C" fn intel_pt_insert_jmx_at_block_start() -> bool {
    STATE.insert_jmx_at_block_start()
}

#[no_mangle]
pub extern "C" fn intel_pt_insert_chain_count_check() -> bool {
    STATE.insert_chain_count_check()
}

#[no_mangle]
pub extern "C" fn intel_pt_trace_guest_pc(pc: u64) {
    STATE.trace_guest_pc(pc);
}

#[no_mangle]
pub extern "C" fn intel_pt_insert_pc_mapping(host_pc: u64, guest_pc: u64) {
    STATE.insert(host_pc, guest_pc);
}

#[no_mangle]
pub extern "C" fn intel_pt_start_recording() {
    STATE.start_recording();
}

#[no_mangle]
pub extern "C" fn intel_pt_stop_recording() {
    STATE.stop_recording();
}

#[no_mangle]
pub extern "C" fn intel_pt_exit() {
    STATE.exit();
}
