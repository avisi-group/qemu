#include <stdio.h>
#include <stdlib.h>

#include "guest_pc.h"

static FILE *trace_file = NULL;
bool guest_pc_disable_direct_chaining = false;

#define FILENAME "guest_pc.trace"

static void init_guest_pc_trace(const char *file_name);


void simple_trace_opt_parse(const char *arg)
{
    init_guest_pc_trace(arg);
}


static void init_guest_pc_trace(const char *file_name)
{
    // Open dump file
    trace_file = fopen(file_name, "w");
    guest_pc_disable_direct_chaining = true; 
    // setbuf(trace_file, NULL);

    if (trace_file == NULL)
    {
        printf("Failed to open simepl trace file for writing");
        exit(-1);
    }
}


void guest_pc_trace_basic_block(long guest_pc)
{
    if (trace_file == NULL) {
        return;
    }

    if (fprintf(trace_file, "%lX\n", guest_pc) == 0) {
        printf("Failed to write guest PC to trace file");
        exit(-1);
    }
}

void guest_pc_close_trace_file(void)
{
    if (trace_file == NULL) {
        return;
    }

    fclose(trace_file);
}