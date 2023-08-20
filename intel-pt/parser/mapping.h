#ifndef INTEL_PT__PARSER__MAPPING_H
#define INTEL_PT__PARSER__MAPPING_H

void init_mapping(void);
void cleanup_mapping(void);
void add_mapping(unsigned long guest_adr, unsigned long host_adr);
unsigned long lookup_mapping(unsigned long host_adr);

#endif