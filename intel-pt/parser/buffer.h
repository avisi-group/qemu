#ifndef INTEL_PT__PARSER__BUFFER_H
#define INTEL_PT__PARSER__BUFFER_H

#include "qemu/osdep.h"
#include "intel-pt/parser/types.h"

void init_buffer(size_t buffer_size);
void cleanup_buffer(void);
void add_data_to_buffer(const unsigned char *buffer, size_t size);
void signal_writing_finished(void);
void wait_for_buffer_to_empty(void);

size_t get_next_job(parser_job_t *job, unsigned char *buffer, size_t job_size, size_t psb_offset);
size_t get_buffer_length(void);

#endif