#include "qemu/osdep.h"
#include "intel-pt/parser/types.h"
#include "intel-pt/parser/buffer.h"

#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>

static unsigned char *buffer = NULL;
static size_t buffer_size = 0;

static volatile size_t head_pos = 0;
static volatile size_t tail_pos = 0;
static volatile size_t amount_of_data_in_buffer = 0;

/* Think of these as a tail pos which never decrease 
 * if we had an infinite buffer, makes parsing easier */
static volatile size_t total_amount_parsed = 0;

static volatile bool writing_finished = false;

static pthread_mutex_t write_lock;
static pthread_mutex_t read_lock;


void init_buffer(size_t _buffer_size) 
{
   buffer = calloc(_buffer_size, sizeof(unsigned char));
   buffer_size = _buffer_size;
}


void cleanup_buffer(void) 
{
   free(buffer);
}


void signal_writing_finished(void)
{
   writing_finished = true;
}

size_t get_buffer_length(void)
{
   return amount_of_data_in_buffer;
}


void wait_for_buffer_to_empty(void)
{
   while(true) {
      pthread_mutex_lock(&read_lock);
      printf("waiting %lu\n", amount_of_data_in_buffer);

      if (buffer_size - amount_of_data_in_buffer > 65536) {
         pthread_mutex_unlock(&read_lock);
         return;
      }
      pthread_mutex_unlock(&read_lock);
   }
}


void add_data_to_buffer(
   const unsigned char *_buffer, size_t size
) {
   /* Todo may want to use zero_chain_count if buffer space starts filling up */
   pthread_mutex_lock(&write_lock);

   size_t remaning_buffer_space = buffer_size - head_pos;

   if (size <= remaning_buffer_space) {
      /* Enough space that we don't need to wrap */
      memcpy(buffer + head_pos, _buffer, size);
      
      head_pos += size;
      amount_of_data_in_buffer += size;

      pthread_mutex_unlock(&write_lock);
      return;
   }

   /* Need to wrap around the buffer */

   /* Copy from head -> end */
   memcpy(buffer + head_pos, _buffer, remaning_buffer_space);

   /* Copy from start -> remaning */
   size_t remaning_data_to_copy = size - remaning_buffer_space;

   memcpy(buffer, _buffer + remaning_buffer_space, remaning_data_to_copy);

   head_pos = remaning_data_to_copy;
   amount_of_data_in_buffer += size;

   pthread_mutex_unlock(&write_lock);
}


size_t get_next_job(
   parser_job_t *job, unsigned char *_buffer, 
   size_t job_size, size_t psb_offset
) {
   pthread_mutex_lock(&read_lock);

   size_t amount_to_copy;
   size_t amount_to_parse;

   while (true) {
      if (writing_finished && tail_pos == head_pos) {
         /* No Data left */
         pthread_mutex_unlock(&read_lock);
         return 0;
      }

      if (job_size + psb_offset < amount_of_data_in_buffer) {
         /* Enough data to make a job */
         pthread_mutex_lock(&write_lock);
         amount_to_copy = job_size + psb_offset;
         amount_to_parse = job_size;
         break;
      }

      if (writing_finished) {
         /* Parse remaning data */
         pthread_mutex_lock(&write_lock);
         amount_to_copy = amount_of_data_in_buffer;
         amount_to_parse = amount_of_data_in_buffer;
         break;
      }
   }

   /* Copy the data */
   if (tail_pos + amount_to_copy <= buffer_size) {
      /* Copy from tail to amount */
      memcpy(_buffer, buffer + tail_pos, amount_to_copy);

      tail_pos += amount_to_parse;
   } else {
      size_t tail_to_end_amount = buffer_size - tail_pos;
      size_t start_to_remaning_amount = amount_to_copy - tail_to_end_amount;

      /* Copy from tail to end */
      memcpy(_buffer, buffer + tail_pos, tail_to_end_amount);

      /* Copy from start to remaning */
      memcpy(_buffer + tail_to_end_amount, buffer, start_to_remaning_amount);

      if (start_to_remaning_amount >= psb_offset) {
         tail_pos = start_to_remaning_amount - psb_offset;
      } else {
         /* Weird edge case where we have copied over the buffer circle but not parsed over it */
         tail_pos = buffer_size - (psb_offset - start_to_remaning_amount);
      }
   }

   /* Update worker job */
   job->start_offset = total_amount_parsed;
   job->end_offset = total_amount_parsed + amount_to_parse;

   // printf("start ofst: %lu end ofst: %lu copy ofst: %lu\n", 
   //    job->start_offset, job->end_offset, total_amount_parsed + amount_to_copy
   // );

   total_amount_parsed += amount_to_parse;
   amount_of_data_in_buffer -= amount_to_parse;

   pthread_mutex_unlock(&write_lock);
   pthread_mutex_unlock(&read_lock);

   return amount_to_copy;
}
