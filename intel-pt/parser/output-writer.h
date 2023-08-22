#ifndef INTEL_PT__PARSER__OUTPUT_WRITER_H
#define INTEL_PT__PARSER__OUTPUT_WRITER_H

#include "qemu/osdep.h"
#include "intel-pt/parser/types.h"

bool init_output_file(const char* file_name);
void close_output_file(void);
void save_job_to_output_file(parser_job_t *job);

#endif