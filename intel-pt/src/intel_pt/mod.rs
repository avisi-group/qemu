use std::sync::mpsc::{Receiver, Sender};

use crate::InnerState;

use {
    crate::STATE,
    libipt::{
        packet::{Packet, PacketDecoder},
        ConfigBuilder, PtErrorCode,
    },
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
        sync::mpsc::{channel, TryRecvError},
        thread::JoinHandle,
    },
    twox_hash::XxHash64,
};

mod ring_buffer;

const NR_AUX_PAGES: usize = 1024;
const NR_DATA_PAGES: usize = 256;

pub struct PtTracer {
    pub perf_file_descriptor: i32,
    thread_handle: JoinHandle<()>,
    /// Host to guest address mapping
    mapping: HashMap<u64, u64, BuildHasherDefault<XxHash64>>,
    sender: Sender<()>,
}

impl PtTracer {
    pub fn lookup(&self, host_pc: u64) -> Option<u64> {
        self.mapping.get(&host_pc).copied()
    }

    pub fn insert_mapping(&mut self, host_pc: u64, guest_pc: u64) {
        self.mapping.insert(host_pc, guest_pc);
        println!("mapping {host_pc:x} {guest_pc:x}");
    }

    pub fn start_recording(&self) {
        if unsafe { perf_event_open_sys::ioctls::ENABLE(self.perf_file_descriptor, 0) } < 0 {
            panic!("failed to start recording");
        }
    }

    pub fn stop_recording(&self) {
        if unsafe { perf_event_open_sys::ioctls::DISABLE(self.perf_file_descriptor, 0) } < 0 {
            panic!("failed to stop recording");
        }
    }

    pub fn init() -> Self {
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

        let perf_file_descriptor = unsafe {
            perf_event_open(
                (&mut pea) as *mut _,
                i32::try_from(process::id()).unwrap(),
                -1,
                -1,
                0,
            )
        };
        if perf_file_descriptor < 0 {
            println!("last OS error: {:?}", std::io::Error::last_os_error());
            panic!("perf_event_open failed {perf_file_descriptor}");
        }

        let (sender, receiver) = channel();

        let handle = std::thread::spawn(move || read_pt_data(receiver, perf_file_descriptor));

        Self {
            perf_file_descriptor,
            thread_handle: handle,
            mapping: HashMap::default(),
            sender,
        }
    }

    pub fn exit(self) {
        let Self {
            thread_handle,
            sender,
            ..
        } = self;

        drop(sender);
        thread_handle.join().unwrap();
    }
}

fn get_intel_pt_perf_type() -> u32 {
    let mut intel_pt_type = File::open("/sys/bus/event_source/devices/intel_pt/type").unwrap();

    let mut buf = String::new();
    intel_pt_type.read_to_string(&mut buf).unwrap();

    buf.trim().parse().unwrap()
}

fn read_pt_data(receiver: Receiver<()>, perf_file_descriptor: i32) {
    let mmap = memmap2::MmapOptions::new()
        .len((NR_DATA_PAGES + 1) * page_size::get())
        .map_raw(perf_file_descriptor)
        .unwrap();

    let header = unsafe { &mut *(mmap.as_mut_ptr() as *mut perf_event_mmap_page) };

    header.aux_offset = header.data_offset + header.data_size;
    header.aux_size = (NR_AUX_PAGES * page_size::get()) as u64;

    let aux_area = memmap2::MmapOptions::new()
        .len(header.aux_size as usize)
        .offset(header.aux_offset)
        .map_raw(perf_file_descriptor)
        .unwrap();

    let mut ring_buffer = RingBuffer::new(mmap, aux_area);

    let mut buf = Vec::new();

    'record_loop: loop {
        if let Err(TryRecvError::Disconnected) = receiver.try_recv() {
            println!("disconnected, exiting");
            break 'record_loop;
        }

        if let Some(record) = ring_buffer.next_record() {
            let mut temp_buf = vec![0; record.data().len()];
            record.data().copy_to_slice(&mut temp_buf);
            buf.extend(temp_buf);
            println!("got record");
        }
    }

    let mut decoder = PacketDecoder::new(&ConfigBuilder::new(&mut buf).unwrap().finish()).unwrap();

    // keep syncing forward until successful
    match decoder.sync_forward() {
        Ok(_) => (),
        Err(e) => match e.code() {
            PtErrorCode::Eos => {
                println!("eos");
            }
            _ => {
                println!("got error while syncing: {e:?}");
            }
        },
    }

    loop {
        println!("getting next packet");
        let p = decoder.next();
        println!("got pkt {p:?}");
        match p {
            Ok(p) => match p {
                Packet::Tip(inner) => {
                    println!("doing lookup");
                    let inner_ptr = unsafe { &*STATE.inner.data_ptr() };
                    if let InnerState::IntelPt(tracer) = inner_ptr {
                        if let Some(guest_pc) = tracer.lookup(inner.tip()) {
                            println!("tip {:x} {:x}", inner.tip(), guest_pc);
                        }
                    }
                }
                Packet::Fup(inner) => {
                    println!("fup {:x}", inner.fup());
                }
                _ => (),
            },
            Err(pkt_error) => {
                println!("packet error {pkt_error:?}");
                if let Err(e) = decoder.sync_forward() {
                    if e.code() == PtErrorCode::Eos {
                        println!("got eos");
                        return;
                    } else {
                        println!("got error while syncing: {e:?}");
                    }
                }
            }
        }
    }
}
