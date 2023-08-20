#ifndef INTEL_PT__PARSER__PT_PARSER_H
#define INTEL_PT__PARSER__PT_PARSER_H

void mapping_parse(
   unsigned char* buffer, unsigned long buffer_size,
   unsigned long start_offset, unsigned long end_offset
);

#endif