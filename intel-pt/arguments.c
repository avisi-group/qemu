
#include "intel-pt/arguments.h"

#include "qemu/osdep.h"
#include "qemu/help_option.h"
#include "qemu/option.h"
#include "qemu/config-file.h"

#include "intel-pt/mapping.h"

static int version;
static bool enabled;

QemuOptsList intel_pt_opts = {
    .name = "intel-pt",
    .implied_opt_name = "intel-pt",
    .merge_lists = true,
    .head = QTAILQ_HEAD_INITIALIZER(intel_pt_opts.head),
    .desc = {
        {   
            .name = "mapping",
            .type = QEMU_OPT_STRING,
        },
        { /* end of list */ }
    },
};

static bool intel_pt_parse_mapping_opt(QemuOpts *opts, const char* opt);


void intel_pt_opt_parse(const char *optarg)
{
    QemuOpts *opts = qemu_opts_parse_noisily(
        qemu_find_opts("intel-pt"), optarg, true
    );

    if (!opts) {
        exit(1);
    }

    enabled = true;

    const bool parsed_mapping = (
        qemu_opt_get(opts, "mapping") && 
        intel_pt_parse_mapping_opt(opts, qemu_opt_get(
            opts, "mapping"
        ))
    );

    if (!parsed_mapping) {
        exit(1);
    }

    qemu_opts_del(opts);

    printf("INTEL_PT: %d\n", version);
}


static bool intel_pt_parse_mapping_opt(QemuOpts *opts, const char* opt) 
{
    if(!init_mapping_file(opt)) {
        return false;
    }

    qemu_opt_get_del(opts, opt);
    
    return true;
}
