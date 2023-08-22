#ifndef INTEL_PT__PARSER__PT_PARSER_H
#define INTEL_PT__PARSER__PT_PARSER_H

#include "intel-pt/parser/types.h"

void mapping_parse(
   unsigned char* buffer, unsigned long buffer_size,
   parser_job_t *current_job 
);

#endif