bool guest_pc_disable_direct_chaining(void);

bool insert_jmx_at_block_start(void);

void trace_guest_pc(uint64_t pc);

void handle_arg_intel_pt(const char *arg);

void intel_pt_start_recording(void);

void intel_pt_stop_recording(void);
