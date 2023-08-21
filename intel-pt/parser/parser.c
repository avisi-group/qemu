
#include "qemu/osdep.h"

#include "intel-pt/config.h"
#include "intel-pt/recording-internal.h"
#include "intel-pt/parser/parser.h"
#include "intel-pt/parser/mapping.h"
#include "intel-pt/parser/pt-parser.h"
#include "intel-pt/parser/types.h"

#include <stdio.h>
#include <stdlib.h>

static unsigned char *temp_buffer = NULL;
static unsigned long buffer_size = 0;
static unsigned long pos_in_buffer = 0;

static FILE* output_file = NULL;

bool init_internal_parsing(
   const char *trace_file_name
) {
   if (intel_pt_config.record_intel_pt_data) {
      fprintf(stderr, "Cannot record intel pt data to file and perform internal parsing at the same time\n");
      return false;
   }

   init_ipt_recording(NULL);

   intel_pt_config.record_intel_pt_data = true;
   intel_pt_config.give_parser_mapping = true;
   intel_pt_config.use_internal_parsing = true;

   temp_buffer = calloc(1073741824, sizeof(char));
   buffer_size = 1073741824;

   init_mapping();

   output_file = fopen(trace_file_name, "w+");

   return true;
}


void save_intel_pt_data(
   const unsigned char *buffer, size_t size
) {
   if (!intel_pt_config.use_internal_parsing) {
      return;
   }

   memcpy(temp_buffer + pos_in_buffer, buffer, size);
   pos_in_buffer += size;
}


void record_parser_mapping(
   unsigned long guest_adr, unsigned long host_adr
) {
   if (!intel_pt_config.use_internal_parsing) {
      return;
   }

   add_mapping(guest_adr, host_adr);
}


void finish_parsing_and_close_file(void)
{
   if (!intel_pt_config.use_internal_parsing) {
      return;
   }

   parser_job_t current_job;

   mapping_parse(
      temp_buffer, pos_in_buffer, 0, pos_in_buffer, &current_job
   );

   for (int i = 0; i < current_job.number_of_elements; ++i) {
      fprintf(output_file, "%lX\n", current_job.trace[i]);
   }

   fclose(output_file);
   cleanup_mapping();  
}


/* Current TODO:
 *    - Improve hashmap implementation to deal with resizing, will this work concurently 
 *    - Get parsing to happen concurrently, biggest problems
 *       - Getting the output to be writen to a file in order
 *       - Workers requesting new work concurrently 
 *    - Get parsing to happen before all data collected, biggest problems  
 *       - Expanding the buffer, also want to shrink buffer when lower data has been cleaned
 *          - Can't modify the buffer whilst workers are parsing 
 *          - Could use a circular buffer that only expands when data fills up or never expands and halts QEMU when it starts getting full 
 *       - Adding data to the buffer whilst workers are parsing 
 */