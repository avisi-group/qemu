#include "scribe/csrc/chain-count.h"
#include "qemu/osdep.h"

static uint32_t *chain_count = NULL;

void init_chain_count_cpu_state(uint32_t *c_count)
{
   chain_count = c_count;
   reset_chain_count();
}

void reset_chain_count(void)
{
   *chain_count = 1000;
}

/*
 * asm to decrement the chain count in the ARMCPUState
 * and to jump back to qemu if the chain count reaches zero
 *
 * decl   0x132d4(%rbp)       # decrement chain count by one
 * cmpl   $0x0,0x132d4(%rbp)  # compare with zero
 * je return addr; if count zero jump out of chaining
 *               ; note the je is not included in this machine code
 *               ' it is added by QEMU using tcg_gen functions
 */
uint8_t chain_count_machine_code[13] ={
   0xFF, 0x8D, 0xD4, 0x32, 0x01, 0x00, 0x83, 0xBD, 0xD4, 0x32, 0x01, 0x00, 0x00
};
unsigned int chain_count_machine_code_length = 13;
