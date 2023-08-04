#include "intel-pt/jmx-jump.h"
#include "intel-pt/config.h"

bool init_jmx_jump(bool enabled) 
{
   intel_pt_config.insert_jmx_at_block_start = enabled;

   if (enabled) {
      /* TODO update the mapping offset to resetent the jump */
      intel_pt_config.mapping_offset = 7;
   }

   return true;
}


uint8_t jmx_machine_code[] = {
   0x48, 0x8d, 0x05, 0x02, 0x00,
   0x00, 0x00, 0xff, 0xd0, 0x48,
   0x83, 0xc4, 0x08,
};


unsigned int jmx_machine_code_length = 13;