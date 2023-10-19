use {
    crate::STATE,
    std::ffi::{c_char, CStr},
};

/// # Safety
///
/// Arg must be a valid pointer to a valid C string
#[no_mangle]
pub unsafe extern "C" fn handle_arg_intel_pt(arg: *const c_char) {
    STATE.handle_arg(unsafe { CStr::from_ptr(arg) }.to_str().unwrap());
}

#[no_mangle]
pub extern "C" fn intel_pt_simple_tracing() -> bool {
    STATE.enable_simple_tracing()
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
pub extern "C" fn intel_pt_insert_pt_write() -> bool {
    STATE.insert_pt_write()
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
pub extern "C" fn intel_pt_pc_mapping(host_pc: u64, guest_pc: u64) {
    STATE.pc_mapping(host_pc, guest_pc);
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
