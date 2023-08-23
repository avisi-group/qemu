#include "qemu/osdep.h"

#include "intel-pt/parser/types.h"
#include "intel-pt/parser/output-writer.h"

#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>

typedef struct queue_element_t { 
   bool is_set;
   parser_job_t job;
} queue_element_t;


static FILE *output_file = NULL;
static queue_element_t *finished_job_queue = NULL;
static unsigned long job_queue_size = 32;
static unsigned long min_trace_pos = 0;
static pthread_mutex_t queue_lock;

bool init_output_file(
   const char* trace_file_name
) {
   output_file = fopen(trace_file_name, "w+");
   finished_job_queue = calloc(job_queue_size, sizeof(queue_element_t));

   return true;
}


void close_output_file(void)
{  
   for (int i = 0; i < job_queue_size; ++i) {
      if (finished_job_queue[i].is_set) {
         fprintf(stderr, "Fatal error reached end of tracing and finnished jobs not writen to file\n");
         exit(-1);
      }
   }

   fclose(output_file);
}


static inline void write_parser_job(parser_job_t *job);


void save_job_to_output_file(
   parser_job_t *job
) {
   pthread_mutex_lock(&queue_lock);

   if (job->start_offset == min_trace_pos) {
      /* We can write this emediatly */
      write_parser_job(job);

      pthread_mutex_unlock(&queue_lock);
      return;
   }

   /* Can't write this job, must wait */
   for (int i = 0; i < job_queue_size; ++i) {
      if (finished_job_queue[i].is_set) {
         continue;
      }

      finished_job_queue[i].is_set = true;
      memcpy(&finished_job_queue[i].job, job, sizeof(parser_job_t));

      break;
   }

   pthread_mutex_unlock(&queue_lock);
}


static inline void write_parser_job(
   parser_job_t *job
){
   /* Write elements */

   /* Todo: could have this occour outside of a lock allowing elemenents to be added during writing */
   for (int i = 0; i < job->number_of_elements; ++i) {
      fprintf(output_file, "%lX\n", job->trace[i]);
   }

   free(job->trace);

   min_trace_pos = job->end_offset;

   /* Check if there are other jobs that can be written */
   for (int i = 0; i < job_queue_size; ++i) {
      if (!finished_job_queue[i].is_set || 
           finished_job_queue[i].job.start_offset != min_trace_pos
         ) {
         continue;
      }

      write_parser_job(&finished_job_queue[i].job);

      finished_job_queue[i].is_set = false;

      return;
   }
}

