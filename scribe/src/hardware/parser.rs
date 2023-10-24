use {
    crate::hardware::{
        decoder::find_next_sync,
        notify::Notify,
        ordered_queue::Sender,
        thread_handle::{Context, ThreadHandle},
        PacketHandler, BUFFER_SIZE,
    },
    bbqueue::Consumer,
    libipt::{packet::PacketDecoder, ConfigBuilder, PtErrorCode},
    rayon::{ThreadPool, ThreadPoolBuilder},
    std::{marker::PhantomData, ops::Range},
};

const MAX_SYNCPOINTS: usize = 64;
const NUM_THREADS: usize = 6;

pub struct Parser {
    handle: ThreadHandle,
}

impl Parser {
    pub fn init<P: PacketHandler>(
        notify: Notify,
        consumer: Consumer<'static, BUFFER_SIZE>,
        queue: Sender<Vec<P::ProcessedPacket>>,
    ) -> Self {
        Self {
            handle: ThreadHandle::spawn(move |ctx| {
                let mut state = ParserState::<P>::init(ctx, notify, consumer, queue);
                state.run();
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

struct ParserState<P: PacketHandler> {
    ctx: Context,
    empty_buffer_notifier: Notify,
    consumer: Consumer<'static, BUFFER_SIZE>,
    queue: Sender<Vec<P::ProcessedPacket>>,
    next_sequence_number: u64,
    pool: ThreadPool,
    terminating: bool,
    _packet_handler: PhantomData<P>,
}

impl<P: PacketHandler> ParserState<P> {
    fn init(
        ctx: Context,
        empty_buffer_notifier: Notify,
        consumer: Consumer<'static, BUFFER_SIZE>,
        queue: Sender<Vec<P::ProcessedPacket>>,
    ) -> Self {
        let celf = Self {
            ctx,
            empty_buffer_notifier,
            consumer,
            queue,

            next_sequence_number: 0,
            pool: ThreadPoolBuilder::new()
                .num_threads(NUM_THREADS)
                .build()
                .unwrap(),
            terminating: false,
            _packet_handler: PhantomData,
        };

        celf.ctx.ready();

        celf
    }

    fn run(&mut self) {
        loop {
            if self.ctx.received_exit() {
                log::trace!("parse terminating");
                self.terminating = true;
            }

            // read data from consumer, checking for shutdown if empty
            let read = match self.consumer.split_read() {
                Ok(read) => read,
                Err(bbqueue::Error::InsufficientSize) => {
                    if self.terminating {
                        log::trace!("insufficient size, terminating");
                        return;
                    } else {
                        log::trace!("notify");
                        self.empty_buffer_notifier.notify();
                        continue;
                    }
                }
                Err(_) => {
                    continue;
                }
            };

            // copy data into local `Vec`
            // TODO: heap allocation + memcpy might be expensive here, currently done to
            // avoid issues with split read
            let mut data = read.bufs().0.to_owned();
            data.extend_from_slice(read.bufs().1);
            let len = read.combined_len();

            // find the range of bytes alinged to sync points
            let range = match find_sync_range(&mut data) {
                Ok(range) => range,
                // found no sync points, skip?
                // TODO: maybe panic here
                Err(ParseError::NoSync) => {
                    log::trace!("skipping {len}");
                    read.release(len);
                    continue;
                }
                // only find a single sync point
                Err(ParseError::OneSync(start)) => {
                    if self.terminating {
                        // parse all remaining bytes even if there isn't a final sync point
                        log::trace!("parsing remaining bytes from {start}");
                        start..data.len()
                    } else {
                        // we don't have enough data so notify for more
                        log::trace!("notify");
                        self.empty_buffer_notifier.notify();
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
            let queue = self.queue.clone();

            let sequence_number = self.next_sequence_number;
            self.next_sequence_number += 1;

            self.pool.spawn(move || {
                // create a new decoder, synchronise it, and then assert that it synchronised to
                // byte 0
                let mut decoder =
                    PacketDecoder::new(&ConfigBuilder::new(&mut data).unwrap().finish()).unwrap();
                decoder.sync_forward().unwrap();
                assert_eq!(decoder.sync_offset().unwrap(), 0);

                let mut packet_handler = P::new();

                loop {
                    match decoder.next() {
                        Ok(p) => packet_handler.process_packet(p),
                        Err(e) => {
                            if e.code() == PtErrorCode::Eos {
                                log::trace!("reached eos");
                                break;
                            } else {
                                log::error!("packet error: {:?}", e);
                                continue;
                            }
                        }
                    }
                }

                // push data into queue to be picked up by writer
                queue.send(sequence_number, packet_handler.finish());
            });
        }
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
