#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <fcntl.h>
#include <string.h>
#include <sys/ioctl.h>
#include <linux/perf_event.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <pthread.h>
#include <stdatomic.h>
#include <time.h>
#include <errno.h>

#include "qemu/osdep.h"
#include "qemu/typedefs.h"
#include "qemu/help_option.h"
#include "qemu/option.h"

#include "intel-pt/config.h"
#include "intel-pt/recording.h"
#include "intel-pt/recording-internal.h"
#include "intel-pt/parser/parser.h"

#define NR_DATA_PAGES 256
#define NR_AUX_PAGES 1024
#define PAGE_SIZE 4096

#define mb() asm volatile("mfence" :: \
                              : "memory")
#define rmb() asm volatile("lfence" :: \
                               : "memory")
#define __READ_ONCE(x) (*(const volatile typeof(x) *)&(x))
#define READ_ONCE(x)    \
    ({                  \
        __READ_ONCE(x); \
    })
#define WRAPPED_OFFSET ((wrapped_tail + OFFSET) % size)

typedef unsigned long u64;

static int get_intel_pt_perf_type(void);
static void *trace_thread_proc(void *arg);
static void set_trace_thead_cpu_affinity(void);

/* File Globals */
int ipt_perf_fd = -1;

static pthread_t trace_thread = 0;

static struct perf_event_mmap_page *header;
static void *base_area, *data_area, *aux_area;
static pid_t pid = 0;

static volatile int stop_thread = 0;
static volatile int recording_thread_started = 0;

volatile int reading_data = 0;


inline void wait_for_pt_thread(void)
{
    while (reading_data) { }
}


inline void ipt_start_recording(void)
{
    wait_for_pt_thread();
    if(intel_pt_config.record_intel_pt_data && ioctl(ipt_perf_fd, PERF_EVENT_IOC_ENABLE) == -1) {
        fprintf(stderr, "Failed to start perf recording reason: %s\n", strerror(errno));
    }
}


inline void ipt_stop_recording(void)
{
    if (intel_pt_config.record_intel_pt_data && ioctl(ipt_perf_fd, PERF_EVENT_IOC_DISABLE)  == -1) {
        fprintf(stderr, "Failed to stop perf recording reason: %s\n", strerror(errno));
    }
}


inline void ipt_breakpoint_call(void)
{
    /* This function is ment to do nothing */
}


bool init_ipt_recording(const char* file_name)
{
    pid = getpid();

    pthread_create(&trace_thread, NULL, trace_thread_proc, (void* __restrict__) file_name);

    while (!recording_thread_started) 
    { 
        /* Wait for the recording thread to start */
    }

    set_trace_thead_cpu_affinity();

    return true;
}


void finish_recording_and_close_file(void)
{
    stop_thread = true;

    while (recording_thread_started) 
    {
        /* Wait for recording thread to stop*/
    }   

    finish_parsing_and_close_file();
}


static void set_trace_thead_cpu_affinity(void) 
{
    cpu_set_t cpuset;
    CPU_ZERO(&cpuset);
    for (int i = 3; i < 6; i++)
        CPU_SET(i, &cpuset);

    if (pthread_setaffinity_np(trace_thread, sizeof(cpuset), &cpuset) != 0)
    {
        fprintf(stderr, "Failed to set trace thread affinity\n");
        exit(EXIT_FAILURE);
    }

    pthread_t curr = pthread_self();

    cpu_set_t cpuset_;
    CPU_ZERO(&cpuset_);
    for (int i = 0; i < 3; i++)
        CPU_SET(i, &cpuset_);

    if (pthread_setaffinity_np(curr, sizeof(cpuset_), &cpuset_) != 0)
    {
        fprintf(stderr, "Failed to set qemu thread affinity\n");
        exit(EXIT_FAILURE);
    }
}


static int setup_perf_fd(struct perf_event_attr* pea);

static void* setup_base_area(void);
static void* setup_aux_area(void);

static void record_pt_data_to_trace_file(char* file_name);
static void record_pt_data_to_internal_memory(void);


static void *trace_thread_proc(void *arg)
{
    // Set-up the perf_event_attr structure
    struct perf_event_attr pea;
    memset(&pea, 0, sizeof(pea));
    pea.size = sizeof(pea);

    // perf event type
    pea.type = get_intel_pt_perf_type();

    // Event should start disabled, and not operate in kernel-mode.
    pea.disabled = 1;
    pea.exclude_kernel = 1;
    pea.exclude_hv = 1;
    pea.precise_ip = 2;

    // 2401 to disable return compression
    pea.config = 0x2001; // 0010000000000001

    ipt_perf_fd = setup_perf_fd(&pea);
    base_area = setup_base_area();

    header = base_area;
    data_area = base_area + header->data_offset;

    header->aux_offset = header->data_offset + header->data_size;
    header->aux_size = NR_AUX_PAGES * PAGE_SIZE;

    aux_area = setup_aux_area();

    char* file_name = (char *) arg;

    if (file_name != NULL) {
        record_pt_data_to_trace_file(file_name);
    } else {
        record_pt_data_to_internal_memory();
    }

    return NULL;
}


static void record_pt_data_to_trace_file(char* file_name) 
{
    const unsigned char *buffer = (const unsigned char *)aux_area;
    u64 size = header->aux_size;
    u64 last_head = 0;

    FILE *ipt_data_file = fopen(file_name, "w+");

    if (ipt_data_file == NULL) {
        fprintf(stderr, "Failed to open file to save intel pt data to\n");
        exit(1);
    }

    recording_thread_started = 1;

    while (true)
    {
        u64 head = READ_ONCE(header->aux_head);
        rmb();

        if (head == last_head)
        {
            if (stop_thread) break;
            else continue;
        }

        reading_data = 1;
        // fprintf(stderr, "STARTING To Read\n");

        u64 wrapped_head = head % size;
        u64 wrapped_tail = last_head % size;

        if (wrapped_head > wrapped_tail)
        {
            // from tail --> head
            fwrite(
                buffer + wrapped_tail,
                wrapped_head - wrapped_tail,
                1, ipt_data_file
            );
        }
        else
        {
            // from tail -> size
            fwrite(
                buffer + wrapped_tail,
                size - wrapped_tail,
                1, ipt_data_file
            );

            // from start --> head
            fwrite(
                buffer, wrapped_head,
                1, ipt_data_file
            );
        }

        last_head = head;

        // fprintf(
        //     stderr, "WRT=%lu WRH=%lu, H=%lu D=%lu\n",
        //     wrapped_tail, wrapped_head, head, wrapped_head > wrapped_tail ?
        //     wrapped_head - wrapped_tail : (size - wrapped_tail) + wrapped_head
        // );

        mb();

        u64 old_tail;

        do
        {
            old_tail = __sync_val_compare_and_swap(&header->aux_tail, 0, 0);
        } while (!__sync_bool_compare_and_swap(&header->aux_tail, old_tail, head));

        reading_data = 0;
    }

    fclose(ipt_data_file);
    
    recording_thread_started = 0;
}


static void record_pt_data_to_internal_memory(void) 
{
    const unsigned char *buffer = (const unsigned char *)aux_area;
    u64 size = header->aux_size;
    u64 last_head = 0;

    recording_thread_started = 1;

    while (true)
    {
        u64 head = READ_ONCE(header->aux_head);
        rmb();

        if (head == last_head)
        {
            if (stop_thread) break;
            else continue;
        }

        reading_data = 1;
        // fprintf(stderr, "STARTING To Read\n");

        u64 wrapped_head = head % size;
        u64 wrapped_tail = last_head % size;

        if (wrapped_head > wrapped_tail)
        {
            // from tail --> head
            save_intel_pt_data(
                buffer + wrapped_tail,
                wrapped_head - wrapped_tail
            );
        }
        else
        {
            // from tail -> size
            save_intel_pt_data(
                buffer + wrapped_tail,
                size - wrapped_tail
            );

            // from start --> head
            save_intel_pt_data(
                buffer, wrapped_tail
            );
        }

        last_head = head;

        // fprintf(
        //     stderr, "WRT=%lu WRH=%lu, H=%lu D=%lu\n",
        //     wrapped_tail, wrapped_head, head, wrapped_head > wrapped_tail ?
        //     wrapped_head - wrapped_tail : (size - wrapped_tail) + wrapped_head
        // );

        mb();

        u64 old_tail;

        do
        {
            old_tail = __sync_val_compare_and_swap(&header->aux_tail, 0, 0);
        } while (!__sync_bool_compare_and_swap(&header->aux_tail, old_tail, head));

        reading_data = 0;
    }
    
    recording_thread_started = 0;
}


static int setup_perf_fd(struct perf_event_attr* pea) 
{
    int fd = syscall(SYS_perf_event_open, pea, pid, -1, -1, 0);
    
    if (fd < 0)
    {
        fprintf(stderr, "intel-pt: could not enable tracing\n");
        fprintf(stderr, "   Errno %i: %s\n", errno, strerror(errno));
        exit(EXIT_FAILURE);
    }

    return fd;
}


static void* setup_base_area(void) 
{
    void* b_area = mmap(
        NULL, (NR_DATA_PAGES + 1) * PAGE_SIZE,
        PROT_READ | PROT_WRITE, MAP_SHARED,
        ipt_perf_fd, 0
    );

    if (b_area == MAP_FAILED)
    {
        close(ipt_perf_fd);

        fprintf(stderr, "intel-pt: could not map data area\n");
        fprintf(stderr, "   Errno %i: %s\n", errno, strerror(errno));
        exit(EXIT_FAILURE);
    }

    return b_area;
}


static void* setup_aux_area(void) 
{
    void* a_area = mmap(
        NULL, header->aux_size,
        PROT_READ | PROT_WRITE, MAP_SHARED,
        ipt_perf_fd, header->aux_offset
    );

    if (a_area == MAP_FAILED)
    {
        munmap(base_area, (NR_DATA_PAGES + 1) * PAGE_SIZE);
        close(ipt_perf_fd);

        fprintf(stderr, "intel-pt: could not map aux area\n");
        fprintf(stderr, "   Errno %i: %s\n", errno, strerror(errno));
        exit(EXIT_FAILURE);
    }

    return a_area;
}


static int get_intel_pt_perf_type(void)
{
    // The Intel PT type is dynamic, so read it from the relevant file.
    int intel_pt_type_fd = open("/sys/bus/event_source/devices/intel_pt/type", O_RDONLY);
    if (intel_pt_type_fd < 0)
    {
        fprintf(stderr, "intel-pt: could not find type descriptor - is intel pt available?\n");
        exit(EXIT_FAILURE);
    }

    char type_number[16] = {0};
    int bytes_read = read(intel_pt_type_fd, type_number, sizeof(type_number) - 1);
    close(intel_pt_type_fd);

    if (bytes_read == 0)
    {
        fprintf(stderr, "intel-pt: type descriptor read error\n");
        exit(EXIT_FAILURE);
    }

    return atoi(type_number);
}

