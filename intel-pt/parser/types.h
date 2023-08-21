#ifndef INTEL_PT__PARSER__TYPES_H
#define INTEL_PT__PARSER__TYPES_H

typedef struct parser_job_t {
   unsigned long start_offset;
   unsigned long end_offset;
   unsigned long *trace;
   unsigned long number_of_elements;
   unsigned long trace_size;
} parser_job_t;

#endif 