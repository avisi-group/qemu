void handle_arg_intel_pt(const char *arg);

bool intel_pt_enable_direct_chaining(void);

bool intel_pt_insert_jmx_at_block_start(void);

void intel_pt_trace_guest_pc(uint64_t pc);

void intel_pt_insert_pc_mapping(uint64_t host_pc, uint64_t guest_pc);

void intel_pt_start_recording(void);

void intel_pt_stop_recording(void);

void intel_pt_exit(void);
