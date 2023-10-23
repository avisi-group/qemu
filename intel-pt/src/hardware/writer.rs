use {
    crate::hardware::{
        ordered_queue::Receiver,
        thread_handle::{Context, ThreadHandle},
        ProcessedPacketHandler,
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
    pub fn init<P: ProcessedPacketHandler, PATH: AsRef<Path>>(
        path: PATH,
        handler_context: P::Ctx,
        queue: Receiver<Vec<P::ProcessedPacket>>,
    ) -> Self {
        let writer = BufWriter::with_capacity(8 * 1024, File::create(path).unwrap());

        let handle = ThreadHandle::spawn(move |thread_ctx| {
            write_pt_data::<P, _>(thread_ctx, writer, queue, handler_context)
        });

        Self { handle }
    }

    pub fn exit(self) {
        self.handle.exit();
    }
}

fn write_pt_data<P: ProcessedPacketHandler, W: Write>(
    thread_ctx: Context,
    mut w: W,
    mut queue: Receiver<Vec<P::ProcessedPacket>>,
    handler_ctx: P::Ctx,
) {
    log::trace!("starting");

    let mut handler = P::new(handler_ctx);

    thread_ctx.ready();

    loop {
        let Some(data) = queue.recv() else {
            if thread_ctx.received_exit() {
                log::trace!("writer terminating");
                w.flush().unwrap();
                return;
            };
            continue;
        };

        data.into_iter()
            .filter_map(|data| handler.calculate_pc(data))
            .for_each(|pc| {
                w.write_all(&pc.to_le_bytes()).unwrap();
            });
    }
}
