
#include "qemu/osdep.h"

#include "intel-pt/config.h"
#include "intel-pt/recording-internal.h"
#include "intel-pt/parser/parser.h"
#include "intel-pt/parser/mapping.h"
#include "intel-pt/parser/pt-parser.h"

#include <stdio.h>
#include <stdlib.h>

static unsigned char *temp_buffer = NULL;
static unsigned long buffer_size = 0;
static unsigned long pos_in_buffer = 0;

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

   mapping_parse(
      temp_buffer, pos_in_buffer, 0, pos_in_buffer
   );

   cleanup_mapping();  
}
