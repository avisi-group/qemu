#include "qemu/osdep.h"

#include "intel-pt/config.h"
#include "intel-pt/pt-write.h"

void init_pt_write(bool enabled)
{
   intel_pt_config.insert_pt_write = enabled;
}