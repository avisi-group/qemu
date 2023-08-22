
#include "qemu/osdep.h"

#include "intel-pt/config.h"
#include "intel-pt/recording-internal.h"
#include "intel-pt/parser/parser.h"
#include "intel-pt/parser/mapping.h"
#include "intel-pt/parser/pt-parser.h"
#include "intel-pt/parser/output-writer.h"
#include "intel-pt/parser/buffer.h"
#include "intel-pt/parser/types.h"

#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>

#define NUMBER_OF_THREADS 2
#define JOB_SIZE     65536
#define PSB_OFFSET   4096

#define BUFFER_SIZE  196608

static pthread_t threads[NUMBER_OF_THREADS];
static void* worker_thread(void* args);

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

   init_buffer(BUFFER_SIZE);
   init_mapping();
   
   if(!init_output_file(trace_file_name)) {
      return false;
   }

   for (int i = 0; i < NUMBER_OF_THREADS; ++i) {
      pthread_create(&threads[i], NULL, worker_thread, NULL);
   }

   return true;
}


void save_intel_pt_data(
   const unsigned char *buffer, size_t size
) {
   if (!intel_pt_config.use_internal_parsing) {
      return;
   }

   add_data_to_buffer(buffer, size);
}


void record_parser_mapping(
   unsigned long guest_adr, unsigned long host_adr
) {
   if (!intel_pt_config.use_internal_parsing) {
      return;
   }

   add_mapping(guest_adr, host_adr);
}


void check_intel_pt_buffer_has_space(void)
{
   if (!intel_pt_config.use_internal_parsing) {
      return;
   }

   if (BUFFER_SIZE - get_buffer_length() < 65536) {
      wait_for_buffer_to_empty();
   }
}


void finish_parsing_and_close_file(void)
{
   if (!intel_pt_config.use_internal_parsing) {
      return;
   }

   signal_writing_finished();

   for (int i = 0; i < NUMBER_OF_THREADS; ++i) {
      pthread_join(threads[i], NULL);
   }

   close_output_file();
   cleanup_mapping();  
   cleanup_buffer();
}


static void* worker_thread(void* args) 
{
   unsigned char *buffer = calloc(PSB_OFFSET + JOB_SIZE, sizeof(unsigned char));
   
   parser_job_t current_job;
   size_t buffer_size;

   while((buffer_size = get_next_job(&current_job, buffer, JOB_SIZE, PSB_OFFSET)) != 0) {
      mapping_parse(buffer, buffer_size, &current_job);

      save_job_to_output_file(&current_job);
   }

   return NULL;
}


/* Current TODO:
 *    - Get parsing to happen before all data collected, biggest problems  
 *       - Will want to use a circular buffer for this, but it should appear lienar to everything else 
 *       - Expanding the buffer, also want to shrink buffer when lower data has been cleaned
 *          - Can't modify the buffer whilst workers are parsing 
 *          - Could use a circular buffer that only expands when data fills up or never expands and halts QEMU when it starts getting full 
 *       - If buffer starts to fill up need to pause QEMU execution 
 *       - Adding data to the buffer whilst workers are parsing 
 *       - Create N - 2 threads at start 
 *       - When writing to buffer finishes can spawn a further 2 threads 
 */