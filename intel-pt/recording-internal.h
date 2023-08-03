#ifndef INTEL_PT__RECORDING_INTERNAL_H_
#define INTEL_PT__RECORDING_INTERNAL_H_

#include "qemu/osdep.h"

bool init_ipt_recording(const char* file_name);
void finish_recording_and_close_file(void);

#endif