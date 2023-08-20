#include "intel-pt/mapping.h"
#include "intel-pt/config.h"
#include "intel-pt/parser/parser.h"

#include "qemu/osdep.h"
#include "qapi/error.h"

static FILE* mapping_file;

bool init_mapping_file(const char* file_name) {
    mapping_file = fopen(file_name, "w+");

    if (!mapping_file) {
        fprintf(stderr, "Failed to create mapping file: %s, reason: %s\n", file_name, strerror(errno));
        return false;
    }

    intel_pt_config.record_mapping = true;

    return true;
}


void record_mapping(unsigned long guest_adr, unsigned long host_adr) {
    if (intel_pt_config.give_parser_mapping) {
        record_parser_mapping(guest_adr, host_adr);
    }

    if (!intel_pt_config.record_mapping) {
        return;
    }

    fprintf(mapping_file, "%lX, %lX\n", guest_adr, host_adr + intel_pt_config.mapping_offset);
}

void close_mapping_file(void) {
    if (!intel_pt_config.record_mapping) {
        return;
    }

    fclose(mapping_file);   
}
