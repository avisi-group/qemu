#include "intel-pt/mapping.h"

#include "qemu/osdep.h"
#include "qapi/error.h"

static FILE* mapping_file;

bool ipt_record_mapping;

bool init_mapping_file(const char* file_name) {
    mapping_file = fopen(file_name, "w");

    if (!mapping_file) {
        return false;
    }

    ipt_record_mapping = true;

    return true;
}


void record_mapping(unsigned long guest_adr, unsigned long host_adr) {
    if (!ipt_record_mapping) {
        return;
    }

    fprintf(mapping_file, "%lX %lX\n", guest_adr, host_adr);
}

void close_mapping_file(void) {
    if (!ipt_record_mapping) {
        return;
    }

    fclose(mapping_file);   
}
