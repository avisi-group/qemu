#include "qemu/osdep.h"

#include "intel-pt/parser/mapping.h"

#include <stdio.h>
#include <stdlib.h>


#define MAPPING_START_SIZE 5000000
#define SIZE_INCREASE_RATE 2

typedef struct mapping_entry_t {
   bool is_set;
   unsigned long host_adr;
   unsigned long guest_adr;
} mapping_entry_t;

typedef struct mapping_t {
   mapping_entry_t *entries;
   unsigned long number_of_slots;
   unsigned long number_of_entries;
} mapping_t;

static mapping_t mapping = {
   .entries = NULL,
   .number_of_slots = 0,
   .number_of_entries = 0,
};

// static void increase_slot_size(void);


void init_mapping(void) 
{
   mapping.entries = calloc(MAPPING_START_SIZE, sizeof(mapping_entry_t));
   mapping.number_of_slots = MAPPING_START_SIZE;
}


void cleanup_mapping(void)
{
   free(mapping.entries);
}


unsigned long lookup_mapping(
   unsigned long host_adr
) {
   for (int i = 0; i < mapping.number_of_slots; ++i) {
      /* Using quadratic probing https://www.geeksforgeeks.org/quadratic-probing-in-hashing/ */
      int hash = (host_adr + i * i) % mapping.number_of_slots; 

      if (mapping.entries[hash].host_adr == host_adr) {
         return mapping.entries[hash].guest_adr;
      }

      if (mapping.entries[hash].is_set) {
         continue;
      }

      /* Not in mapping */
      break;
   }

   /* Not in mapping */
   return 0;
}


void add_mapping(
   unsigned long guest_adr, unsigned long host_adr
) {
   for (int i = 0; i < mapping.number_of_slots; ++i) {
      /* Using quadratic probing https://www.geeksforgeeks.org/quadratic-probing-in-hashing/ */
      int hash = (host_adr + i * i) % mapping.number_of_slots; 

      if (mapping.entries[hash].is_set) {
         continue;
      }

      mapping.entries[hash].is_set = true;
      mapping.entries[hash].guest_adr = guest_adr;
      mapping.entries[hash].host_adr = host_adr;

      break;
   }

   // if (mapping.number_of_entries >= mapping.number_of_slots * 2) {
   //    increase_slot_size();
   // }  
}


// static void increase_slot_size(void)
// {
//    mapping_entry_t *new_entries = (mapping_entry_t*) calloc(
//       mapping.number_of_slots * SIZE_INCREASE_RATE, sizeof(mapping_entry_t)
//    );

//    memcpy(new_entries, mapping.entries, mapping.number_of_slots * sizeof(mapping_entry_t));

//    free(mapping.entries);

//    mapping.entries = new_entries;
//    mapping.number_of_slots = mapping.number_of_slots * SIZE_INCREASE_RATE;
// }