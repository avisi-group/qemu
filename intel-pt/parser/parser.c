
#include "qemu/osdep.h"

#include "intel-pt/config.h"
#include "intel-pt/parser/parser.h"
#include "intel-pt/parser/mapping.h"


bool init_internal_parsing(
   const char *trace_file_name
) {
   /* Todo: if intel-pt-data set cancel this will cause issues */

   init_mapping();

   return false;
}


void save_intel_pt_data(
   const unsigned char *buffer, size_t size
) {

}


void record_parser_mapping(
   unsigned long guest_adr, unsigned long host_adr
) {
   add_mapping(guest_adr, host_adr);
}


void finish_parsing_and_close_file(void)
{
   cleanup_mapping();  
}
