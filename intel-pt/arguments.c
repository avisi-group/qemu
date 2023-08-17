#include "qemu/osdep.h"
#include "qemu/help_option.h"
#include "qemu/option.h"
#include "qemu/config-file.h"

#include "intel-pt/arguments.h"
#include "intel-pt/mapping.h"
#include "intel-pt/chain-count.h"
#include "intel-pt/jmx-jump.h"
#include "intel-pt/recording-internal.h"
#include "intel-pt/pt-write.h"
#include "intel-pt/config.h"

#include <string.h>


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
        {
            .name = "intel-pt-data",
            .type = QEMU_OPT_STRING,
        },
        {
            .name = "insert-jmx",
            .type = QEMU_OPT_BOOL
        },
        {
            .name = "use-chain-count",
            .type = QEMU_OPT_BOOL
        },
        {
            .name = "insert-pt-write",
            .type = QEMU_OPT_BOOL
        },
        { /* end of list */ }
    },
};


static bool parse_mapping_opt(QemuOpts *opts, const char* opt);
static bool parse_intel_pt_data_opt(QemuOpts *opts, const char* opt);
static bool parse_chain_count_opt(QemuOpts *opts, const char* opt);
static bool parse_jmx_at_block_start_opt(QemuOpts *opts, const char* opt);
static bool parse_pt_write_opt(QemuOpts *opts, const char* opt);

#define FALSE_OPT 0
#define TRUE_OPT 1
#define ERR_OPT 2

static int parse_true_false(const char* opt);


void intel_pt_opt_parse(const char *optarg)
{
    QemuOpts *opts = qemu_opts_parse_noisily(
        qemu_find_opts("intel-pt"), optarg, true
    );

    if (!opts) {
        fprintf(stderr, "Failed to find intel-pt opts\n");
        exit(1);
    }

    if (qemu_opt_get(opts, "mapping")) {
        const bool handled_mapping = parse_mapping_opt(
            opts, qemu_opt_get(opts, "mapping")
        );

        if (!handled_mapping) {
            fprintf(stderr, "Failed to handle intel-pt mapping argument\n");
            exit(1);
        }
    }


    if (qemu_opt_get(opts, "intel-pt-data")) {
        const bool handled_ipt_data = parse_intel_pt_data_opt(
            opts, qemu_opt_get(opts, "intel-pt-data")
        );

        if (!handled_ipt_data) {
            fprintf(stderr, "Failed to handle intel-pt intel-pt-data argument\n");
            exit(1);
        }
    }


    if (qemu_opt_get(opts, "insert-jmx")) {
        const bool handled_insert_jmx = parse_jmx_at_block_start_opt(
            opts, qemu_opt_get(opts, "insert-jmx")
        );

        if (!handled_insert_jmx) {
            fprintf(stderr, "Failed to handle intel-pt insert-jmx argument\n");
            exit(1);
        }
    }

    if (qemu_opt_get(opts, "use-chain-count")) {
        const bool handled_use_chain_count = parse_chain_count_opt(
            opts, qemu_opt_get(opts, "use-chain-count")
        );

        if (!handled_use_chain_count) {
            fprintf(stderr, "Failed to handle intel-pt use-chain-count argument\n");
            exit(1);
        }
    }

    if (qemu_opt_get(opts, "insert-pt-write")) {
        const bool handled_insert_pt_write = parse_pt_write_opt(
            opts, qemu_opt_get(opts, "insert-pt-write")
        );

        if (!handled_insert_pt_write) {
            fprintf(stderr, "Failed to handle intel-pt insert-pt-write argument\n");
            exit(1);
        }
    }

    qemu_opts_del(opts);
}


static bool parse_mapping_opt(QemuOpts *opts, const char* opt) 
{
    if(!init_mapping_file(opt)) {
        return false;
    }

    qemu_opt_get_del(opts, opt);
    
    return true;
}


static bool parse_intel_pt_data_opt(QemuOpts *opts, const char* opt) 
{
    if(!init_ipt_recording(opt)) {
        return false;
    }

    qemu_opt_get_del(opts, opt);

    return true;
}


static bool parse_chain_count_opt(QemuOpts *opts, const char* opt) 
{
    switch (parse_true_false(opt))
    {
    case TRUE_OPT:
        init_chan_count(true);
        break;
    case FALSE_OPT:
        init_chan_count(false);
        break;
    case ERR_OPT:
        fprintf(stderr, "Value must be either 'true' or 'false'\n");
        return false;
    }

    return true;
}


static bool parse_jmx_at_block_start_opt(QemuOpts *opts, const char* opt)
{
    switch (parse_true_false(opt))
    {
    case TRUE_OPT:
        init_jmx_jump(true);
        break;
    case FALSE_OPT:
        init_jmx_jump(false);
        break;
    case ERR_OPT:
        fprintf(stderr, "Value must be either 'true' or 'false'\n");
        return false;
    }

    return true;
}

static bool parse_pt_write_opt(QemuOpts *opts, const char* opt)
{
    switch (parse_true_false(opt))
    {
    case TRUE_OPT:
        init_pt_write(true);
        break;
    case FALSE_OPT:
        init_pt_write(false);
        break;
    case ERR_OPT:
        fprintf(stderr, "Value must be either 'true' or 'false'\n");
        return false;
    }

    return true;
}


static int parse_true_false(const char* opt)
{
    if (strcmp(opt, "true") == 0) {
        return TRUE_OPT;
    }

    if (strcmp(opt, "false") == 0) {
        return FALSE_OPT;
    }

    return ERR_OPT;
}
