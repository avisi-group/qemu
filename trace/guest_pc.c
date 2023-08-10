#include <stdio.h>
#include <stdlib.h>

#include "guest_pc.h"

static FILE *trace_file = NULL;

#define FILENAME "guest_pc.trace"

void init_guest_pc_trace(void)
{
    // Open dump file
    trace_file = fopen(FILENAME, "w");
    setbuf(trace_file, NULL);

    if (trace_file == NULL)
    {
        printf("Failed to open " FILENAME " for writing");
        exit(-1);
    }
}

void guest_pc_trace_basic_block(long guest_pc)
{
    if (fwrite(&guest_pc, sizeof(long), 1, trace_file) != 1) {
        printf("Failed to write guest PC to trace file");
        exit(-1);
    }
}
