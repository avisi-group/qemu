use crate::intel_pt::thread_handle::Context;
use {
    crate::{
        intel_pt::{
            decoder::find_next_sync, notify::Notify, thread_handle::ThreadHandle, ParsedData,
            BUFFER_SIZE,
        },
        Mode,
    },
    bbqueue::Consumer,
    libipt::{
        packet::{Compression, Packet, PacketDecoder},
        ConfigBuilder, PtErrorCode,
    },
    parking_lot::Mutex,
    rayon::ThreadPoolBuilder,
    std::{collections::BinaryHeap, ops::Range, sync::Arc},
};

const MAX_SYNCPOINTS: usize = 64;
const NUM_THREADS: usize = 8;

pub struct Parser {
    handle: ThreadHandle,
}

impl Parser {
    pub fn init(
        notify: Notify,
        consumer: Consumer<'static, BUFFER_SIZE>,
        queue: Arc<Mutex<BinaryHeap<ParsedData>>>,
        mode: Mode,
    ) -> Self {
        Self {
            handle: ThreadHandle::spawn(move |ctx| {
                ParserState::new(ctx, notify, consumer, queue, mode).run()
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

struct ParserState {
    ctx: Context,
    empty_buffer_notifier: Notify,
    consumer: Consumer<'static, BUFFER_SIZE>,
    queue: Arc<Mutex<BinaryHeap<ParsedData>>>,
    mode: Mode,
    last_ip: u64,
    next_sequence_number: u64,
}

impl ParserState {
    fn new(
        ctx: Context,
        empty_buffer_notifier: Notify,
        consumer: Consumer<'static, BUFFER_SIZE>,
        queue: Arc<Mutex<BinaryHeap<ParsedData>>>,
        mode: Mode,
    ) -> Self {
        Self {
            ctx,
            empty_buffer_notifier,
            consumer,
            queue,
            mode,
            last_ip: 0,
            next_sequence_number: 0,
        }
    }

    fn run(&mut self) {
        let mut terminating = false;

        let pool = ThreadPoolBuilder::new()
            .num_threads(NUM_THREADS)
            .build()
            .unwrap();

        self.ctx.ready();

        loop {
            if self.ctx.received_exit() {
                log::trace!("parse terminating");
                terminating = true;
            }

            // read data from consumer, checking for shutdown if empty
            let read = match self.consumer.split_read() {
                Ok(read) => read,
                Err(bbqueue::Error::InsufficientSize) => {
                    if terminating {
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

            let mut data = read.bufs().0.to_owned();
            data.extend_from_slice(read.bufs().1);
            let len = read.combined_len();

            let range = match self.find_sync_range(&mut data) {
                Ok(range) => range,
                Err(ParseError::NoSync) => {
                    // found no sync points, skip
                    log::trace!("skipping {len}");
                    read.release(len);
                    continue;
                }
                Err(ParseError::OneSync(start)) => {
                    if terminating {
                        // parse all remaining bytes even if there isn't a final sync point
                        log::trace!("parsing remaining bytes from {start}");
                        self.parse_slice(&mut data[start..]);
                        return;
                    }

                    log::trace!("notify");
                    self.empty_buffer_notifier.notify();
                    continue;
                }
            };

            if self.mode != Mode::PtWrite {
                panic!();
            }

            read.release(range.end);
            assert_eq!(0, range.start);
            data.truncate(range.end);

            let queue = self.queue.clone();
            let sequence_number = self.next_sequence_number;
            self.next_sequence_number += 1;
            pool.spawn(move || {
                let mut decoder =
                    PacketDecoder::new(&ConfigBuilder::new(&mut data).unwrap().finish()).unwrap();
                decoder.sync_forward().unwrap();
                assert_eq!(decoder.sync_offset().unwrap(), 0);

                let mut pcs = vec![];

                loop {
                    match decoder.next() {
                        Ok(p) => match p {
                            Packet::Ptw(inner) => {
                                pcs.push(inner.payload());
                            }
                            _ => (),
                        },
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

                queue.lock().push(ParsedData {
                    sequence_number,
                    data: pcs,
                });
            });

            // log::trace!("parsing range {range:?}");
            // self.parse_slice(&mut read.buf_mut()[range]);
        }
    }

    fn find_sync_range(&mut self, slice: &mut [u8]) -> Result<Range<usize>, ParseError> {
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

    ///
    fn parse_slice(&mut self, slice: &mut [u8]) {
        let mut decoder = PacketDecoder::new(&ConfigBuilder::new(slice).unwrap().finish()).unwrap();
        decoder.sync_forward().unwrap();
        assert_eq!(decoder.sync_offset().unwrap(), 0);

        let mut pcs = vec![];

        loop {
            match decoder.next() {
                Ok(p) => match self.mode {
                    Mode::Tip => match p {
                        Packet::Tip(inner) => {
                            let ip = match inner.compression() {
                                Compression::Suppressed => continue,
                                Compression::Update16 => (self.last_ip >> 16) << 16 | inner.tip(),
                                Compression::Update32 => (self.last_ip >> 32) << 32 | inner.tip(),
                                Compression::Update48 => (self.last_ip >> 32) << 32 | inner.tip(),
                                Compression::Sext48 => (((inner.tip() as i64) << 16) >> 16) as u64,
                                Compression::Full => inner.tip(),
                            };

                            self.last_ip = ip;

                            pcs.push(ip - 9);
                        }
                        _ => (),
                    },
                    Mode::Fup => todo!(),
                    Mode::PtWrite => match p {
                        Packet::Ptw(inner) => {
                            pcs.push(inner.payload());
                        }
                        _ => (),
                    },
                    _ => unreachable!(),
                },

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

        self.send_data(pcs);
    }

    fn send_data(&mut self, data: Vec<u64>) {
        self.queue.lock().push(ParsedData {
            sequence_number: self.next_sequence_number,
            data,
        });

        self.next_sequence_number += 1;
    }
}
