use {
    crate::{
        hardware::{
            ordered_queue::Receiver,
            thread_handle::{Context, ThreadHandle},
            SharedPcMap,
        },
        Mode,
    },
    std::{
        fs::File,
        io::{BufWriter, Write},
        path::Path,
    },
};

pub struct Writer {
    handle: ThreadHandle,
}

impl Writer {
    pub fn init<P: AsRef<Path>>(
        path: P,
        pc_map: SharedPcMap,
        queue: Receiver<Vec<u64>>,
        mode: Mode,
    ) -> Self {
        let writer = BufWriter::with_capacity(8 * 1024, File::create(path).unwrap());

        let handle =
            ThreadHandle::spawn(move |ctx| write_pt_data(ctx, writer, queue, pc_map, mode));

        Self { handle }
    }

    pub fn exit(self) {
        self.handle.exit();
    }
}

fn write_pt_data<W: Write>(
    ctx: Context,
    mut w: W,
    mut queue: Receiver<Vec<u64>>,
    pc_map: SharedPcMap,
    mode: Mode,
) {
    log::trace!("starting");

    ctx.ready();

    loop {
        let Some(data) = queue.recv() else {
            if ctx.received_exit() {
                log::trace!("writer terminating");
                w.flush().unwrap();
                return;
            };
            continue;
        };

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
