
#include "qemu/osdep.h"

#include "intel-pt/config.h"
#include "intel-pt/recording-internal.h"
#include "intel-pt/parser/parser.h"
#include "intel-pt/parser/mapping.h"
#include "intel-pt/parser/pt-parser.h"
#include "intel-pt/parser/output-writer.h"
#include "intel-pt/parser/types.h"

#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>

#define NUMBER_OF_THREADS 6
#define MIN_JOB_SIZE 131072

/* Saving Variables */
static unsigned char *temp_buffer = NULL;
static unsigned long buffer_size = 0;
static volatile unsigned long pos_in_buffer = 0;

/* Parsing Varaibles */
static volatile unsigned long start_of_unparsed = 0;
static volatile bool writing_finished = false; 
static pthread_mutex_t job_request_lock;


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

   return init_output_file(trace_file_name);
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


static void* worker_thread(void* args);
static bool get_next_job(parser_job_t *job);


void finish_parsing_and_close_file(void)
{
   if (!intel_pt_config.use_internal_parsing) {
      return;
   }

   writing_finished = true;

   pthread_t threads[NUMBER_OF_THREADS];

   for (int i = 0; i < NUMBER_OF_THREADS; ++i) {
      pthread_create(&threads[i], NULL, worker_thread, NULL);
   }

   for (int i = 0; i < NUMBER_OF_THREADS; ++i) {
      pthread_join(threads[i], NULL);
   }

   close_output_file();
   cleanup_mapping();  
}


static void* worker_thread(void* args) 
{
   parser_job_t current_job;

   while(get_next_job(&current_job)) {
      mapping_parse(temp_buffer, pos_in_buffer, &current_job);

      save_job_to_output_file(&current_job);
   }

   return NULL;
}


static bool get_next_job(parser_job_t *job)
{
   pthread_mutex_lock(&job_request_lock);

   if (writing_finished && pos_in_buffer == start_of_unparsed) {
      pthread_mutex_unlock(&job_request_lock);
      return false;
   }

   while (
      start_of_unparsed - pos_in_buffer < MIN_JOB_SIZE && 
      !writing_finished
   ) {
      /* Wait for data to be avaliable */
   }

   /* Data avliable get next job */
   job->start_offset = start_of_unparsed;
   job->end_offset = MIN(start_of_unparsed + MIN_JOB_SIZE, pos_in_buffer);

   start_of_unparsed += job->end_offset - job->start_offset;
   
   pthread_mutex_unlock(&job_request_lock);

   return true;
}


/* Current TODO:
 *    - Improve hashmap implementation to deal with resizing, will this work concurently 
 *    - Get parsing to happen concurrently, biggest problems
 *       - Workers requesting new work concurrently 
 *    - Get parsing to happen before all data collected, biggest problems  
 *       - Will want to use a circular buffer for this, but it should appear lienar to everything else 
 *       - Expanding the buffer, also want to shrink buffer when lower data has been cleaned
 *          - Can't modify the buffer whilst workers are parsing 
 *          - Could use a circular buffer that only expands when data fills up or never expands and halts QEMU when it starts getting full 
 *       - Adding data to the buffer whilst workers are parsing 
 */