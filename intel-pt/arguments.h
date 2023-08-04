#ifndef INTEL_PT__ARGUMENTS_H
#define INTEL_PT__ARGUMENTS_H

#include "qemu/osdep.h"
#include "qemu/option.h"

extern QemuOptsList intel_pt_opts;

void intel_pt_opt_parse(const char *optarg);

#endif