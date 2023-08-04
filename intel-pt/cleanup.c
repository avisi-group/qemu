#include "intel-pt/cleanup.h"
#include "intel-pt/mapping.h"
#include "intel-pt/recording-internal.h"

void intel_pt_cleanup(void) {
   close_mapping_file();
   finish_recording_and_close_file();
}