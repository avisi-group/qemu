use {
    crate::hardware::{
        decoder::find_next_sync,
        notify::Notify,
        ordered_queue::Sender,
        thread_handle::{Context, ThreadHandle},
        PacketParser, BUFFER_SIZE,
    },
    bbqueue::Consumer,
    libipt::{packet::PacketDecoder, ConfigBuilder, PtErrorCode},
    rayon::ThreadPoolBuilder,
    std::{
        ops::Range,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    },
};

/// Number of Intel PT synchronisation points included in each work item
const MAX_SYNCPOINTS: usize = 128;
/// Number of threads
const NUM_THREADS: usize = 6;
/// Pending work queue depth per thread
const THREAD_WORK_QUEUE_DEPTH: usize = 4096;
/// Maximum number of in-flight tasks
const MAX_TASKS: usize = NUM_THREADS * THREAD_WORK_QUEUE_DEPTH;

pub struct Parser {
    handle: ThreadHandle,
}

impl Parser {
    pub fn init<P: PacketParser>(
        empty_buffer_notifier: Notify,
        writer_ready_notifier: Notify,
        consumer: Consumer<'static, BUFFER_SIZE>,
        queue: Sender<Vec<P::ProcessedPacket>>,
    ) -> Self {
        Self {
            handle: ThreadHandle::spawn(move |ctx| {
                run_parser::<P>(
                    ctx,
                    empty_buffer_notifier,
                    writer_ready_notifier,
                    consumer,
                    queue,
                );
            }),
        }
    }

    pub fn exit(self) {
        self.handle.exit();
    }
}

enum ParseError {
    /// Failed to parse slice as no sync points were found
    NoSync,
    /// Failed to parse slice as only a single sync point ({0:?}) was found
    OneSync(usize),
}

fn run_parser<P: PacketParser>(
    ctx: Context,
    empty_buffer_notifier: Notify,
    writer_ready_notifier: Notify,
    mut consumer: Consumer<'static, BUFFER_SIZE>,
    queue: Sender<Vec<P::ProcessedPacket>>,
) {
    let mut next_sequence_number = 0;
    let mut terminating = false;

    let pool = ThreadPoolBuilder::new()
        .num_threads(NUM_THREADS)
        .build()
        .unwrap();

    let task_count = Arc::new(AtomicUsize::new(0));

    //let mut w =
    // std::io::BufWriter::new(std::fs::File::create("/mnt/tmp/ptdump").unwrap());

    ctx.ready();

    loop {
        if ctx.received_exit() {
            log::trace!("parse terminating");
            terminating = true;
        }

        // read data from consumer, checking for shutdown if empty
        let read = match consumer.split_read() {
            Ok(read) => read,
            Err(bbqueue::Error::InsufficientSize) => {
                if terminating {
                    log::info!("parser terminating");

                    while task_count.load(Ordering::Relaxed) != 0 {}

                    writer_ready_notifier.wait();

                    return;
                } else {
                    log::trace!("notify");
                    empty_buffer_notifier.notify();
                    continue;
                }
            }
            Err(e) => {
                log::trace!("error from split_read: {:?}", e);
                continue;
            }
        };

        // for dumping raw PT data
        // w.write_all(read.bufs().0).unwrap();
        // w.write_all(read.bufs().1).unwrap();
        // read.release(usize::MAX);
        // continue;

        // limit maximum number of tasks in flight
        while task_count.load(Ordering::Relaxed) > MAX_TASKS {}

        // copy data into local `Vec`
        // TODO: heap allocation + memcpy might be expensive here, currently done to
        // avoid issues with split read
        let mut data = read.bufs().0.to_owned();
        data.extend_from_slice(read.bufs().1);

        // find the range of bytes alinged to sync points
        let range = match find_sync_range(&mut data) {
            Ok(range) => range,

            Err(ParseError::NoSync) => {
                panic!("found no sync points");
            }

            // only find a single sync point
            Err(ParseError::OneSync(start)) => {
                if terminating {
                    // parse all remaining bytes even if there isn't a final sync point
                    let range = start..data.len();
                    log::warn!("parsing remaining bytes in {range:?}");
                    range
                } else {
                    // we don't have enough data so notify for more
                    log::trace!("notify");
                    empty_buffer_notifier.notify();
                    continue;
                }
            }
        };

        // now we can release all bytes up to that end point
        read.release(range.end);

        // trim our local buffer to size
        data.truncate(range.end);

        // not technically necessary, but since it always holds, not holding is probably
        // a bug?
        assert_eq!(0, range.start);

        // cloning to move into the closure
        let queue = queue.clone();
        let tc_cloned = task_count.clone();

        let sequence_number = next_sequence_number;
        next_sequence_number += 1;

        task_count.fetch_add(1, Ordering::Relaxed);

        pool.spawn(move || task_fn::<P>(queue, data, tc_cloned, sequence_number));
    }
}

/// Finds the syncpoints in the supplied slice, returned as a range
/// containing up to `MAX_SYNCPOINTS` syncpoints
fn find_sync_range(slice: &mut [u8]) -> Result<Range<usize>, ParseError> {
    let Some(start) = find_next_sync(slice) else {
        // slice did not contain any sync points
        return Err(ParseError::NoSync);
    };

    let mut syncpoints = vec![start];

    loop {
        if syncpoints.len() == MAX_SYNCPOINTS {
            break;
        }

        let last = syncpoints.last().unwrap();

        if last + 1 >= slice.len() {
            break;
        }

        match find_next_sync(&mut slice[last + 1..]) {
            Some(syncpoint) => {
                syncpoints.push(syncpoint + last + 1);
            }
            None => break,
        }
    }

    if syncpoints.len() == 1 {
        // slice only contained a single sync point
        return Err(ParseError::OneSync(syncpoints[0]));
    }

    Ok(syncpoints[0]..*syncpoints.last().unwrap())
}

fn task_fn<P: PacketParser>(
    queue: Sender<Vec<P::ProcessedPacket>>,
    mut data: Vec<u8>,
    task_count: Arc<AtomicUsize>,
    sequence_number: u64,
) {
    // create a new decoder, synchronise it, and then assert that it synchronised to
    // byte 0
    let mut decoder = PacketDecoder::new(&ConfigBuilder::new(&mut data).unwrap().finish()).unwrap();
    decoder.sync_forward().unwrap();
    assert_eq!(decoder.sync_offset().unwrap(), 0);

    let mut packet_handler = P::new();

    let result = decoder
        .map(|r| r.map(|p| packet_handler.process(p)))
        .collect::<Result<(), _>>();

    if let Err(e) = result {
        if e.code() != PtErrorCode::Eos {
            panic!("error while decoding: {e}");
        }
    }

    // push processed data into queue to be picked up by writer
    queue.send(sequence_number, packet_handler.finish());

    task_count.fetch_sub(1, Ordering::Relaxed);
}
