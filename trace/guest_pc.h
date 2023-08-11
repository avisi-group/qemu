#pragma once

#include "qemu/osdep.h"

extern bool guest_pc_disable_direct_chaining;

void simple_trace_opt_parse(const char *arg);

void guest_pc_trace_basic_block(long guest_pc);
void guest_pc_close_trace_file(void);
