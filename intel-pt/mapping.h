#ifndef INTEL_PT__MAPPING_H
#define INTEL_PT__MAPPING_H

#include "qemu/osdep.h"

bool init_mapping_file(const char* file_name);
void record_mapping(unsigned long guest_adr, unsigned long host_adr);
void close_mapping_file(void);

#endif