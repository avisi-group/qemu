use {
    crate::{
        intel_pt::{
            thread_handle::{Context, ThreadHandle},
            ParsedData, SharedPcMap,
        },
        Mode,
    },
    parking_lot::Mutex,
    std::{
        collections::BinaryHeap,
        fs::File,
        io::{BufWriter, Write},
        path::Path,
        sync::Arc,
    },
};

pub struct Writer {
    handle: ThreadHandle,
}

impl Writer {
    pub fn init<P: AsRef<Path>>(
        path: P,
        pc_map: SharedPcMap,
        mode: Mode,
    ) -> (Self, Arc<Mutex<BinaryHeap<ParsedData>>>) {
        let writer = BufWriter::with_capacity(8 * 1024, File::create(path).unwrap());
        let priority_queue = Arc::new(Mutex::new(BinaryHeap::<ParsedData>::new()));
        let pq_clone = priority_queue.clone();

        let handle =
            ThreadHandle::spawn(move |ctx| write_pt_data(ctx, writer, pq_clone, pc_map, mode));

        (Self { handle }, priority_queue)
    }

    pub fn exit(self) {
        self.handle.exit();
    }
}

fn write_pt_data<W: Write>(
    ctx: Context,
    mut w: W,
    queue: Arc<Mutex<BinaryHeap<ParsedData>>>,
    pc_map: SharedPcMap,
    mode: Mode,
) {
    log::trace!("starting");
    let mut next_sequence_num = 0;

    ctx.ready();

    loop {
        let ParsedData { data, .. } = {
            let mut guard = queue.lock();

            let Some(ParsedData {
                sequence_number, ..
            }) = guard.peek()
            else {
                if ctx.received_exit() {
                    log::trace!("writer terminating");
                    w.flush().unwrap();
                    return;
                };
                continue;
            };

            log::trace!("sequence_number {sequence_number}");

            if *sequence_number != next_sequence_num {
                continue;
            }

            guard.pop().unwrap()
        };

        next_sequence_num += 1;

        match mode {
            Mode::Tip => {
                data.into_iter()
                    .filter_map(|pc| pc_map.read().get(&pc).copied())
                    .for_each(|pc| {
                        w.write_all(&pc.to_le_bytes()).unwrap();
                    });
            }
            Mode::Fup => {
                data.into_iter()
                    .filter_map(|pc| pc_map.read().get(&pc).copied())
                    .for_each(|pc| {
                        w.write_all(&pc.to_le_bytes()).unwrap();
                    });
            }
            Mode::PtWrite => {
                data.into_iter().for_each(|pc| {
                    w.write_all(&pc.to_le_bytes()).unwrap();
                });
            }
            _ => todo!(),
        }
    }
}
