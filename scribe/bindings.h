/**
 * Size of the Intel PT data buffer in bytes
 */
#define BUFFER_SIZE ((256 * 1024) * 1024)

/**
 * # Safety
 *
 * Arg must be a valid pointer to a valid C string
 */
void handle_arg_scribe(const char *arg);

bool scribe_simple_tracing(void);

bool scribe_insert_jmx_at_block_start(void);

bool scribe_insert_pt_write(void);

bool scribe_insert_chain_count_check(void);

void scribe_trace_guest_pc(uint64_t pc);

void scribe_pc_mapping(uint64_t host_pc, uint64_t guest_pc);

void scribe_start_recording(void);

void scribe_stop_recording(void);

void scribe_exit(void);
