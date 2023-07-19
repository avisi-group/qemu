#ifndef INTEL_PT__RECORDING_H_
#define INTEL_PT__RECORDING_H_

inline void wait_for_pt_thread(void);
inline void ipt_start_recording(void);
inline void ipt_stop_recording(void);
inline void ipt_breakpoint_call(void);

#endif
