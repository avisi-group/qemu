#pragma once

#include "qemu/osdep.h"

void init_chain_count_cpu_state(uint32_t *chain_count);
void reset_chain_count(void);
void zero_chain_count(void);

extern uint8_t chain_count_machine_code[];
extern unsigned int chain_count_machine_code_length;
