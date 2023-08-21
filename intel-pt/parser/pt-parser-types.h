#ifndef INTEL_PT__PARSER__PT_PARSER_TYPES_H_
#define INTEL_PT__PARSER__PT_PARSER_TYPES_H_

#include "intel-pt/parser/types.h"

typedef enum pt_packet_type {  
    TNT,
    TIP,
    TIP_OUT_OF_CONTEXT,
    PIP,
    MODE,
    TRACE_STOP,
    CBR,
    TSC,
    MTC,
    TMA,
    VMCS,
    OVF,
    CYC,
    PSB,
    PSBEND,
    MNT,
    PAD,
    PTW,
    EXSTOP,
    MWAIT,
    PWRE,
    PWRX,
    BBP,
    BIP,
    BEP,
    CFE,
    EVD,
    UNKOWN
} pt_packet_type;


typedef enum pt_tip_type {
    TIP_TIP,
    TIP_PGE,
    TIP_PGD,
    TIP_FUP,
} pt_tip_type;


typedef struct tip_packet_data {
    pt_tip_type type;
    unsigned char ip_bits;
    unsigned char last_ip_use;
    unsigned long ip_buffer;
    unsigned long ip;
} tip_packet_data;


typedef struct pt_packet_t {
   pt_packet_type type;
   union {
      tip_packet_data tip_data;
      unsigned long ptw_packet_data;
   };
} pt_packet_t;


typedef struct pt_state_t {
    /* The current instruction poitner (ip) value */
    unsigned long current_ip;

    /* The last guest ip found */
    unsigned long previous_guest_ip;

   /* Store the last TIP ip value. This is used for 
    * for generating the next value */
   unsigned long last_tip_ip;

   /* Store the last seen intel pt packet */
   pt_packet_t *last_packet;

   /* Keeps track if we are currently waiting for a psbend */
   bool in_psb;

   /* Keeps track if we have seen an fup packet and are 
    * waiting to bind that packet */
   bool in_fup;

   /* Keeps track if the previous packet was a mode. This is used 
    * by fup packets to check if it binds to that mode */
   bool last_was_mode;

   /* Keeps track if the previous packet was a OVF. This is used 
    * by fup packets to indicate a reset after an overflow */
   bool last_was_ovf;

   /* We need to know this as intel pt may send us a
    * fup that takes us back in time */
   bool last_ip_had_mapping;

   /* Track the amount of data needing to be parced*/
   unsigned long size;

   /* Track the current offset in the trace file */
   unsigned long offset;

   /* Concurrent start and end */
   unsigned long start_offset;
   unsigned long end_offset;

   /* buffer to store the raw intelpt data */
   unsigned char *buffer;

   /* current possition in the buffer */
   unsigned int pos_in_buffer;

   /* Used only when getting tip packets */
   unsigned long packet_only_last_tip_ip;

   /* Used for logging the output */
   parser_job_t *current_job;
} pt_state_t;

#endif


