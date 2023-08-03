#ifndef INTEL_PT__JMX_JUMP_H_
#define INTEL_PT__JMX_JUMP_H_

#include "qemu/osdep.h"

bool init_jmx_jump(bool enabled);

extern uint8_t jmx_machine_code[];
extern unsigned int jmx_machine_code_length;

#endif