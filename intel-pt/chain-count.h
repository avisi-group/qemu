#ifndef INTEL_PT__CHAIN_COUNT_H_
#define INTEL_PT__CHAIN_COUNT_H_

#include "qemu/osdep.h"

bool init_chan_count(bool enabled);

void init_chain_count_cpu_state(uint32_t *chain_count);
void reset_chain_count(void);

extern uint8_t chan_count_machine_code[];
extern unsigned int chan_count_machine_code_length;

#endif