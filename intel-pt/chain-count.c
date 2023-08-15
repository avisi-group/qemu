#include "intel-pt/chain-count.h"
#include "intel-pt/config.h"

#include "qemu/osdep.h"

#include <stdio.h>

static uint32_t *chain_count = NULL;


bool init_chan_count(bool enabled)
{
   intel_pt_config.insert_chain_count_check = true;

   return true;
}

void init_chain_count_cpu_state(uint32_t *c_count)
{
   chain_count = c_count;
   *chain_count = 1000;
}

void reset_chain_count(void) 
{  
   *chain_count = 1000;
}


/* 
 * asm to decrement the chain count in the ARMCPUState
 * and to jump back to qemu if the chain count reaches zero
 * 
 * decd 78516[rbp]; decrement chain count by one 
 * cmpd 78516[rbp], 0; compare with zero 
 * je return addr; if count zero jump out of chaining 
 *               ; note the je is not included in this machine code
 *               ' it is added by QEMU using tcg_gen functions
 */
uint8_t chan_count_machine_code[13] ={ 
   0xFF, 0x8D, 0xB4, 0x32, 0x01, 0x00, 0x83, 0xBD, 0xB4, 0x32, 0x01, 0x00, 0x00  
};
unsigned int chan_count_machine_code_length = 13;