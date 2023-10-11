use {
    crate::intel_pt::{ring_buffer::RingBuffer, thread_handle::ThreadHandle, BUFFER_SIZE},
    bbqueue::Producer,
    perf_event_open_sys::bindings::perf_event_mmap_page,
    std::sync::mpsc::Receiver,
};

const NR_AUX_PAGES: usize = 1024;
const NR_DATA_PAGES: usize = 256;

pub struct Reader {
    handle: ThreadHandle,
}

impl Reader {
    pub fn init(perf_file_descriptor: i32, producer: Producer<'static, BUFFER_SIZE>) -> Self {
        Self {
            handle: ThreadHandle::spawn(move |rx| read_pt_data(rx, perf_file_descriptor, producer)),
        }
    }

    pub fn exit(self) {
        self.handle.exit();
    }
}

fn read_pt_data(
    shutdown_receiver: Receiver<()>,
    perf_file_descriptor: i32,
    mut producer: Producer<BUFFER_SIZE>,
) {
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

    loop {
        match ring_buffer.next_record() {
            Some(record) => {
                let mut grant = producer
                    .grant_exact(record.data().len())
                    .expect(&format!("failed to grant {}", record.data().len()));
                // let Ok(mut grant) = producer.grant_exact(record.data().len()) else {
                //     use std::io::Write;
                //     {
                //         std::fs::File::create("/home/fm208/data/ringbuffer_ptwrite.dump")
                //             .unwrap()
                //             .write_all(consumer.read().unwrap().buf())
                //             .unwrap();
                //     }
                //     panic!();
                // };
                record.data().copy_to_slice(grant.buf());
                grant.commit(record.data().len());
            }
            None => {
                if let Ok(()) = shutdown_receiver.try_recv() {
                    log::trace!("read terminating");
                    return;
                }
            }
        }
    }
}
