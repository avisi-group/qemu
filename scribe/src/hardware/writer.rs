use std::sync::{atomic::AtomicU32, Arc};

use crate::hardware::{notify::Notify, reader::NUM_THREADS};

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
        sync::atomic::Ordering,
    },
};

/// Pending work queue depth per thread
const THREAD_WORK_QUEUE_DEPTH: usize = 4096;
/// Maximum number of in-flight tasks
const MAX_TASKS: usize = NUM_THREADS * THREAD_WORK_QUEUE_DEPTH;

pub struct Writer {
    handle: ThreadHandle,
}

impl Writer {
    pub fn init<P: PacketWriter, PATH: AsRef<Path>>(
        path: PATH,
        handler_context: P::Ctx,
        queue: Receiver<Vec<P::ProcessedPacket>>,
        ready_notifier: Notify,
        task_count: Arc<AtomicU32>,
    ) -> Self {
        let writer = BufWriter::new(File::create(path).unwrap());

        let handle = ThreadHandle::spawn(move |thread_ctx| {
            write_pt_data::<P, _>(
                thread_ctx,
                writer,
                queue,
                handler_context,
                ready_notifier,
                task_count,
            )
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
    task_count: Arc<AtomicU32>,
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

            if task_count.load(Ordering::Relaxed) < MAX_TASKS as u32 {
                ready_notifier.notify();
            }

            continue;
        };

        data.into_iter()
            .filter_map(|data| handler.calculate_pc(data))
            .for_each(|pc| {
                w.write_all(&pc.to_le_bytes()).unwrap();
            });
    }
}
