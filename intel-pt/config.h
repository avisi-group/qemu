#ifndef INTEL_PT__ARGUMENTS_H
#define INTEL_PT__ARGUMENTS_H

#include "qemu/osdep.h"

typedef struct IntelPTConfig {
   bool record_mapping;
   int mapping_offset; /* Makes it easier to switch between using block address and the address of the jmx*/
   bool record_intel_pt_data;
   bool insert_chain_count_check;
   bool insert_jmx_at_block_start;
   bool insert_pt_write;
} IntelPTConfig;

extern IntelPTConfig intel_pt_config;

#endif