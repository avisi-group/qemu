#include "qemu/osdep.h"

#include "intel-pt/parser/pt-parser.h"
#include "intel-pt/parser/pt-parser-oppcode.h"
#include "intel-pt/parser/pt-parser-types.h"
#include "intel-pt/parser/mapping.h"
#include "intel-pt/parser/types.h"

#include <stdlib.h>
#include <stdio.h>

// #define DEBUG_MODE_
#define DEBUG_TIME_

#ifdef DEBUG_MODE_ 
#define printf_debug(...); printf(__VA_ARGS__);
#else 
#define printf_debug(...);
#endif

#define LEFT(n) ((state->size - state->offset) >= n)
#define ADVANCE(n) \
   state->offset += n; \
   state->pos_in_buffer += n;
#define GET_BYTES(buffer, size) \
   memcpy(buffer, state->buffer + state->pos_in_buffer, n);
#define INIT_BUFFER(name, size) \
   unsigned char *name = state->buffer + state->pos_in_buffer;

#define LOWER_BITS(value, n) (value & ((1 << n) - 1))
#define MIDDLE_BITS(value, uppwer, lower) (value & (((1 << uppwer) - 1) << lower))

#define RETURN_IF(x) \
    if(x(state, packet)) return true
#define RETURN_IF_2(x, y) \
    if(x(state, packet, y)) return true

#define TRACE_START_LENGTH 20000

static inline void advance_to_first_psb(pt_state_t *state);
static inline bool try_get_next_packet(pt_state_t *state, pt_packet_t *packet);
static inline void handle_tip(pt_state_t *state);
static inline void update_current_ip(pt_state_t *state, unsigned long ip);
static inline void log_basic_block(pt_state_t *state, unsigned long guest_ip);

void mapping_parse(
   unsigned char* buffer, unsigned long buffer_size,
   unsigned long start_offset, unsigned long end_offset,
   parser_job_t *current_job 
) {
   pt_state_t state;
   pt_packet_t packet;

   current_job->start_offset = start_offset;
   current_job->end_offset = end_offset;
   current_job->trace = calloc(TRACE_START_LENGTH, sizeof(unsigned long));
   current_job->number_of_elements = 0;
   current_job->trace_size = TRACE_START_LENGTH;

   memset(&state, 0, sizeof(pt_state_t));
   memset(&packet, 0, sizeof(pt_packet_t));

   state.buffer = buffer;
   state.offset = start_offset;
   state.size = buffer_size;
   state.start_offset = start_offset;
   state.end_offset = end_offset;
   state.current_job = current_job;
   state.pos_in_buffer = start_offset;

   advance_to_first_psb(&state);

   while(try_get_next_packet(&state, &packet)) {
      state.last_packet = &packet;

      if(packet.type == PSB) {
         state.in_psb = true;

         if (state.offset > state.end_offset) {
            break;
         }
      } else if(packet.type == PSBEND) {
         state.in_psb = false;
      } else if(packet.type == TIP) {
         handle_tip(&state);
      }

      state.last_was_mode = false;
      state.last_was_ovf = false;

      if(packet.type == MODE) {
         state.last_was_mode = true;
      } else if(packet.type == OVF) {
         state.last_was_ovf = true;
      }
   }

   if(state.previous_guest_ip != 0) {
      // Record the last basic block which may have not been saved yet
      log_basic_block(&state, state.previous_guest_ip);
   }
}



static void advance_to_first_psb(
   pt_state_t *state
) {
   pt_packet_t packet;

   while(try_get_next_packet(state, &packet)) {
      if (packet.type == PSB) {
         state->in_psb = true;
         break;
      }
   }
}



static inline void handle_tip(
   pt_state_t *state
) {
   bool was_in_fup = false;
   tip_packet_data *tip_data = &state->last_packet->tip_data;

   if(tip_data->type == TIP_FUP &&
         !(state->last_was_mode || state->last_was_ovf)
      ) {
      // We have found an unbound FUP packet. Expecting to 
      // to see a pgd packet to bind to this one 
      state->in_fup = true;
   }


   if((tip_data->type == TIP_PGD || tip_data->type == TIP_PGE) && 
        state->in_fup) {
      // We have found an a PGD packet which binds to the 
      // previous FUP packet 
      state->in_fup = false;
      was_in_fup = true;
   }


   if(state->in_fup) { // Cannot update current ip
      printf_debug("  IN FUP\n");
      return;
   }

   if(was_in_fup && state->last_ip_had_mapping && 
      state->last_tip_ip == tip_data->ip && 
      state->last_tip_ip == state->current_ip
      ) {
        // Want to remove the last ip from the record has we will
        // reach it again. This may not be entierly true tbh 
        printf_debug("  NOTE: Removing previous block from save\n");
        state->previous_guest_ip = 0;
    }


   if(state->current_ip == tip_data->ip && 
      state->last_tip_ip == state->current_ip && 
      tip_data->type == TIP_FUP && state->in_psb) {
      // We have resived a refresh of the current ip, but it 
      // is the same as the current. This will cuase a log to 
      // occour twice, we don't want that. 
      // update_ip = false;
      return;
   }

   // Cab update current ip
   state->last_tip_ip = tip_data->ip;
   update_current_ip(state, tip_data->ip);
}


static inline void update_current_ip(
   pt_state_t *state, unsigned long ip
) {
   state->current_ip = ip;

   unsigned long guest_ip = lookup_mapping(ip);

   if (guest_ip == 0) {
      state->last_ip_had_mapping = false;
      return;
   }

   state->last_ip_had_mapping = true;

   log_basic_block(state, guest_ip);
}


static inline void log_basic_block(
   pt_state_t *state, unsigned long guest_ip
)  {
   if(state->previous_guest_ip == 0) {
      state->previous_guest_ip = guest_ip;
      return;
   } 

   if (state->current_job->number_of_elements >= state->current_job->trace_size - 1) {
      unsigned long *new_trace = calloc(state->current_job->trace_size * 2, sizeof(unsigned long));

      memcpy(new_trace, state->current_job->trace, state->current_job->trace_size * sizeof(unsigned long));

      free(state->current_job->trace);

      state->current_job->trace = new_trace;
      state->current_job->trace_size = state->current_job->trace_size * 2;      
   }

   state->current_job->trace[state->current_job->number_of_elements] = state->previous_guest_ip;
   state->current_job->number_of_elements += 1;
   state->previous_guest_ip = guest_ip;
}


static inline bool parse_psb(pt_state_t *state, pt_packet_t *packet);
static inline bool parse_psb_end(pt_state_t *state, pt_packet_t *packet);
static inline bool parse_tip(pt_state_t *state, pt_packet_t *packet, unsigned long curr_ip) ;
static inline bool parse_pip(pt_state_t *state, pt_packet_t *packet);
static inline bool parse_mode(pt_state_t *state, pt_packet_t *packet);
static inline void parse_unkown(pt_state_t *state, pt_packet_t *packet);

static bool try_get_next_packet(
   pt_state_t *state, pt_packet_t *packet
){
   if (state->offset >= state->size) {
      return false;
   }

   RETURN_IF(parse_psb);
   RETURN_IF(parse_psb_end);
   RETURN_IF_2(parse_tip, state->packet_only_last_tip_ip);
   RETURN_IF(parse_pip);
   RETURN_IF(parse_mode);

   parse_unkown(state, packet);

   return true;
}


static inline bool parse_psb(
   pt_state_t *state, pt_packet_t *packet
) {
   if(!LEFT(PSB_PACKET_LENGTH))
      return false;

   INIT_BUFFER(buffer, PSB_PACKET_LENGTH);

   char expected_buffer[] = PSB_PACKET_FULL;

   if(memcmp(buffer, expected_buffer, PSB_PACKET_LENGTH) != 0)
      return false;

   ADVANCE(PSB_PACKET_LENGTH);

   packet->type = PSB;

   return true;
}


static inline bool parse_psb_end(
   pt_state_t *state, pt_packet_t *packet
) {
   if(!LEFT(PSB_END_PACKET_LENGTH))
        return false;

   INIT_BUFFER(buffer, PSB_END_PACKET_LENGTH);

   if(buffer[0] != OPPCODE_STARTING_BYTE || 
      buffer[1] != PSB_END_OPPCODE)
      return false;

   ADVANCE(PSB_END_PACKET_LENGTH);

   packet->type = PSBEND;

   return true;
}


static inline bool parse_tip_type(unsigned char *buffer, pt_tip_type *type);
static inline bool parse_tip_ip_use(unsigned char ip_bits, unsigned char* last_ip_use);

static inline bool parse_tip(
   pt_state_t *state, pt_packet_t *packet, unsigned long curr_ip
) {
   if(!LEFT(TIP_PACKET_LENGTH))
      return false;

   INIT_BUFFER(buffer, TIP_PACKET_LENGTH);

   // Get the type of this packet 
   pt_tip_type type;

   if(!parse_tip_type(buffer, &type)) return false;

   // Check if the ip is within context
   unsigned char ip_bits = buffer[0] >> 5;

   if(ip_bits == 0b000) {
      ADVANCE(1);
      packet->type = TIP_OUT_OF_CONTEXT;
      return true;
   }

   // ip in context get compression status
   unsigned char last_ip_use;

   if(!parse_tip_ip_use(ip_bits, &last_ip_use)) return false;

   // Create ip buffer
   unsigned long ip_buffer = 0;
   unsigned long ip = curr_ip;

   for(int i = 0; i < 8; i++) {
      unsigned char byte = i >= last_ip_use ? 
         buffer[8 - i] : 
         (curr_ip >> ((7 - i) * 8)) & 0xff;

      ip = (ip << 8) | byte;

      if(i >= last_ip_use)
         ip_buffer = (ip_buffer << 8) | byte;
   }

   // Finished return packet
   ADVANCE(TIP_PACKET_LENGTH - last_ip_use);

   packet->tip_data.type = type;
   packet->tip_data.ip_bits = ip_bits;
   packet->tip_data.last_ip_use = last_ip_use;
   packet->tip_data.ip = ip;
   packet->type = TIP;

   state->packet_only_last_tip_ip = packet->tip_data.ip;

   return true;
}  


static inline bool parse_tip_type(
   unsigned char *buffer, pt_tip_type *type
) {
   unsigned char bits = LOWER_BITS(buffer[0], TIP_OPPCODE_LENGTH_BITS);

   switch (bits) {
   case TIP_BASE_OPPCODE: { 
      *type = TIP_TIP;
      return true; 
   } case TIP_PGE_OPPCODE: { 
      *type = TIP_PGE;
      return true;
   } case TIP_PGD_OPPCODE: { 
      *type = TIP_PGD;
      return true; 
   } case TIP_FUP_OPPCODE: { 
      *type = TIP_FUP;
      return true;
   } default:
      return false;
   }
}


static inline bool parse_tip_ip_use(
   unsigned char ip_bits, unsigned char* last_ip_use
) {
   switch (ip_bits) {
   case 0b001: {
      *last_ip_use = 6;
      return true;
   } case 0b010: {
      *last_ip_use = 4;
      return true;
   } case 0b011: {
#ifdef DEBUG_MODE_ 
      printf("TIP - Not implemented\n");
#endif
      return false;
   } case 0b100: {
      *last_ip_use = 2;
      return true;   
   } case 0b110: {
      *last_ip_use = 0;
      return true;
   } default: {
#ifdef DEBUG_MODE_ 
      printf("TIP - Reserved bits\n");
#endif
      return false;
   } }
}


static inline bool parse_pip(pt_state_t *state, pt_packet_t *packet)
{
   if(!LEFT(PIP_PACKET_LENGTH))
      return false;
   
   INIT_BUFFER(buffer, PIP_PACKET_LENGTH);

   if(buffer[0] != OPPCODE_STARTING_BYTE ||
      buffer[1] != PIP_OPPCODE)
      return false;

   ADVANCE(PIP_PACKET_LENGTH);

   packet->type = PIP;

   return true;
}


static inline bool parse_mode(pt_state_t *state, pt_packet_t *packet)
{
   if(!LEFT(MODE_PACKET_LENGTH))    
      return false;
   
   INIT_BUFFER(buffer, MODE_PACKET_LENGTH);

   if(buffer[0] != MODE_OPPCODE)
      return false;

   // Todo: Parse the two different types of mode

   ADVANCE(MODE_PACKET_LENGTH);

   packet->type = MODE;

   return true;
}


static inline void parse_unkown(
   pt_state_t *state, pt_packet_t *packet
) { 
   ADVANCE(1);
   packet->type = UNKOWN;
}