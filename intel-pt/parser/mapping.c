#include "qemu/osdep.h"

#include "intel-pt/parser/mapping.h"

#include <stdio.h>
#include <stdlib.h>


#define MAPPING_START_SIZE 2000
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

static inline void increase_slot_size(void);
static inline void insert_into_mapping(mapping_t *mapping, unsigned long guest_adr, unsigned long host_adr);


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
      /* Using linear probing */
      int hash = (host_adr + i ) % mapping.number_of_slots; 

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
   insert_into_mapping(&mapping, guest_adr, host_adr);

   mapping.number_of_entries += 1;

   if (mapping.number_of_entries * 2 >= mapping.number_of_slots ) {
      increase_slot_size();
   }  
}


static void increase_slot_size(void)
{
   mapping_t temp_mapping = {
      .entries = calloc(
         mapping.number_of_slots * SIZE_INCREASE_RATE, sizeof(mapping_entry_t)
      ),
      .number_of_slots = mapping.number_of_slots * SIZE_INCREASE_RATE
   };

   for (int i = 0; i < mapping.number_of_slots; ++i) {
      if (!mapping.entries[i].is_set) {
         continue;
      }

      insert_into_mapping(
         &temp_mapping, mapping.entries[i].guest_adr, mapping.entries[i].host_adr
      );
   }

   free(mapping.entries);

   mapping.entries = temp_mapping.entries;
   mapping.number_of_slots = temp_mapping.number_of_slots;
}


static inline void insert_into_mapping(
   mapping_t *mapping, unsigned long guest_adr, unsigned long host_adr
) {
   for (int i = 0; i < mapping->number_of_slots; ++i) {
      /* Using linear probing */
      int hash = (host_adr + i) % mapping->number_of_slots; 

      if (mapping->entries[hash].is_set) {
         continue;
      }

      mapping->entries[hash].is_set = true;
      mapping->entries[hash].guest_adr = guest_adr;
      mapping->entries[hash].host_adr = host_adr;

      return;
   }

   fprintf(stderr, "Fatal error in " __FILE__ " hash map ran out of space\n");
   exit(1);
}