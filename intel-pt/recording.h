#ifndef INTEL_PT__RECORDING_H_
#define INTEL_PT__RECORDING_H_

void wait_for_pt_thread(void);
void ipt_start_recording(void);
void ipt_stop_recording(void);
void ipt_breakpoint_call(void);

#endif
