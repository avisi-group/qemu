#ifndef INTEL_PT__PARSER__PARSER_H
#define INTEL_PT__PARSER__PARSER_H

#include "qemu/osdep.h"


bool init_internal_parsing(
   const char *trace_file_name
);

void save_intel_pt_data(
   const unsigned char *buffer, size_t size
);

void record_parser_mapping(
   unsigned long guest_adr, unsigned long host_adr
);


void finish_parsing_and_close_file(void);
#endif