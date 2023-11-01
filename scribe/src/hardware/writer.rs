use crate::hardware::notify::Notify;

use {
    crate::hardware::{
        ordered_queue::Receiver,
        thread_handle::{Context, ThreadHandle},
        PacketWriter,
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
    pub fn init<P: PacketWriter, PATH: AsRef<Path>>(
        path: PATH,
        handler_context: P::Ctx,
        queue: Receiver<Vec<P::ProcessedPacket>>,
        ready_notifier: Notify,
    ) -> Self {
        let writer = BufWriter::new(File::create(path).unwrap());

        let handle = ThreadHandle::spawn(move |thread_ctx| {
            write_pt_data::<P, _>(thread_ctx, writer, queue, handler_context, ready_notifier)
        });

        Self { handle }
    }

    pub fn exit(self) {
        self.handle.exit();
    }
}

fn write_pt_data<P: PacketWriter, W: Write>(
    thread_ctx: Context,
    mut w: W,
    mut queue: Receiver<Vec<P::ProcessedPacket>>,
    handler_ctx: P::Ctx,
    ready_notifier: Notify,
) {
    log::trace!("starting");

    let mut handler = P::new(handler_ctx);

    thread_ctx.ready();

    loop {
        let Some(data) = queue.recv() else {
            if thread_ctx.received_exit() {
                log::info!("writer terminating");
                assert!(queue.is_empty());
                w.flush().unwrap();
                return;
            };

            ready_notifier.notify();

            continue;
        };

        data.into_iter()
            .filter_map(|data| handler.calculate_pc(data))
            .for_each(|pc| {
                w.write_all(&pc.to_le_bytes()).unwrap();
            });
    }
}
