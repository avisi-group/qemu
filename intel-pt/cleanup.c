
#include "intel-pt/cleanup.h"
#include "intel-pt/mapping.h"

void intel_pt_cleanup(void) {
   close_mapping_file();
}