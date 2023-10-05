use libipt::{
    packet::{Packet, PacketDecoder},
    AddrFilter, Config, ConfigBuilder,
};
use {
    once_cell::sync::Lazy,
    parking_lot::RwLock,
    perf_event_open_sys::{
        bindings::{perf_event_attr, perf_event_mmap_page},
        perf_event_open,
    },
    ring_buffer::RingBuffer,
    std::{
        collections::HashMap,
        fs::File,
        hash::BuildHasherDefault,
        io::Read,
        process,
        sync::atomic::{AtomicI32, Ordering},
    },
    twox_hash::XxHash64,
};

mod ring_buffer;

const NR_AUX_PAGES: usize = 1024;
const NR_DATA_PAGES: usize = 256;

/// Host to guest address mapping
/// Replace with DashMap if RwLock too slow
static _MAPPINGS: Lazy<RwLock<HashMap<u64, u64, BuildHasherDefault<XxHash64>>>> =
    Lazy::new(|| RwLock::new(HashMap::default()));

static PERF_FILE_DESCRIPTOR: AtomicI32 = AtomicI32::new(-1);

#[no_mangle]
pub extern "C" fn intel_pt_start_recording() {
    let res = unsafe {
        perf_event_open_sys::ioctls::ENABLE(PERF_FILE_DESCRIPTOR.load(Ordering::SeqCst), 0)
    };

    if res < 0 {
        panic!("failed to start recording");
    }
}

#[no_mangle]
pub extern "C" fn intel_pt_stop_recording() {
    let res = unsafe {
        perf_event_open_sys::ioctls::DISABLE(PERF_FILE_DESCRIPTOR.load(Ordering::SeqCst), 0)
    };

    if res < 0 {
        panic!("failed to stop recording");
    }
}

pub fn init() {
    let mut pea: perf_event_attr = perf_event_attr::default();

    // perf event type
    pea.type_ = get_intel_pt_perf_type();

    // Event should start disabled, and not operate in kernel-mode.
    pea.set_disabled(1);
    pea.set_exclude_kernel(1);
    pea.set_exclude_hv(1);
    pea.set_precise_ip(2);

    // 2401 to disable return compression
    pea.config = 0x2001; // 0010000000000001

    pea.size = std::mem::size_of::<perf_event_attr>() as u32;

    let fd = unsafe {
        perf_event_open(
            (&mut pea) as *mut _,
            i32::try_from(process::id()).unwrap(),
            -1,
            -1,
            0,
        )
    };
    if fd < 0 {
        println!("last OS error: {:?}", std::io::Error::last_os_error());
        panic!("perf_event_open failed {fd}");
    }
    PERF_FILE_DESCRIPTOR.store(fd, Ordering::Relaxed);

    //base_area = setup_base_area();

    let _handle = std::thread::spawn(move || {
        let mmap = memmap2::MmapOptions::new()
            .len((NR_DATA_PAGES + 1) * page_size::get())
            .map_raw(fd)
            .unwrap();

        let header = unsafe { &mut *(mmap.as_mut_ptr() as *mut perf_event_mmap_page) };

        header.aux_offset = header.data_offset + header.data_size;
        header.aux_size = (NR_AUX_PAGES * page_size::get()) as u64;

        let aux_area = memmap2::MmapOptions::new()
            .len(header.aux_size as usize)
            .offset(header.aux_offset)
            .map_raw(fd)
            .unwrap();

        let mut sampler = RingBuffer::new(mmap, aux_area);
        loop {
            if let Some(record) = sampler.next_record() {
                let mut buf = vec![0; record.data().len()];
                record.data().copy_to_slice(&mut buf);

                let mut decoder =
                    PacketDecoder::new(&ConfigBuilder::new(&mut buf).unwrap().finish()).unwrap();
                if let Err(e) = decoder.sync_forward() {
                    println!("Failed to sync forward: {e}");
                    continue;
                }

                decoder.for_each(|p| println!("got packet {:?}", p));
            }
        }
    });
}

fn get_intel_pt_perf_type() -> u32 {
    let mut intel_pt_type = File::open("/sys/bus/event_source/devices/intel_pt/type").unwrap();

    let mut buf = String::new();
    intel_pt_type.read_to_string(&mut buf).unwrap();

    buf.trim().parse().unwrap()
}
