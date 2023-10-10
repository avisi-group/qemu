use {
    crate::{
        intel_pt::{thread_handle::ThreadHandle, ParsedData, BUFFER_SIZE},
        Mode,
    },
    bbqueue::Consumer,
    libipt::{
        packet::{Compression, Packet, PacketDecoder},
        ConfigBuilder, PtErrorCode,
    },
    parking_lot::Mutex,
    std::{
        collections::BinaryHeap,
        sync::{mpsc::Receiver, Arc},
    },
};

pub struct Parser {
    handle: ThreadHandle,
}

impl Parser {
    pub fn init(
        consumer: Consumer<'static, BUFFER_SIZE>,
        queue: Arc<Mutex<BinaryHeap<ParsedData>>>,
        mode: Mode,
    ) -> Self {
        Self {
            handle: ThreadHandle::spawn(move |rx| {
                ParserState::new(rx, consumer, queue, mode).run()
            }),
        }
    }

    pub fn exit(self) {
        self.handle.exit();
    }
}

fn find_next_sync_point(slice: &mut [u8]) -> Option<usize> {
    let mut decoder = PacketDecoder::new(&ConfigBuilder::new(slice).unwrap().finish()).unwrap();

    decoder
        .sync_forward()
        .ok()
        .map(|_| decoder.sync_offset().unwrap() as usize)
}

enum ParseError {
    /// Failed to parse slice as no sync points were found
    NoSync,
    /// Failed to parse slice as only a single sync point ({0:?}) was found
    OneSync(usize),
}

struct ParserState {
    shutdown_receiver: Receiver<()>,
    consumer: Consumer<'static, BUFFER_SIZE>,
    queue: Arc<Mutex<BinaryHeap<ParsedData>>>,
    mode: Mode,
    last_ip: u64,
    next_sequence_number: u32,
}

impl ParserState {
    fn new(
        shutdown_receiver: Receiver<()>,
        consumer: Consumer<'static, BUFFER_SIZE>,
        queue: Arc<Mutex<BinaryHeap<ParsedData>>>,
        mode: Mode,
    ) -> Self {
        Self {
            shutdown_receiver,
            consumer,
            queue,
            mode,
            last_ip: 0,
            next_sequence_number: 0,
        }
    }

    fn run(&mut self) {
        let mut terminating = false;

        loop {
            if let Ok(()) = self.shutdown_receiver.try_recv() {
                log::trace!("parse terminating");
                terminating = true;
            }

            // read data from consumer, checking for shutdown if empty
            let mut read = match self.consumer.read() {
                Ok(read) => read,
                Err(bbqueue::Error::InsufficientSize) => {
                    if terminating {
                        log::trace!("insufficient size, terminating");
                        return;
                    } else {
                        // log::trace!("notify");
                        // empty_buffer_notifier.notify();
                        continue;
                    }
                }
                Err(_) => {
                    continue;
                }
            };

            let len = read.buf().len();
            log::trace!("read {}", len);

            match self.parse_single_sync(read.buf_mut()) {
                Ok(idx) => {
                    log::trace!("finished, idx: {idx}");
                    read.release(idx);
                }
                Err(ParseError::NoSync) => {
                    // found no sync points, skip
                    log::trace!("skipping {len}");
                    read.release(len);
                }
                Err(ParseError::OneSync(start)) => {
                    if terminating {
                        // parse all remaining bytes even if there isn't a final sync point
                        log::trace!("parsing remaining bytes from {start}");
                        self.parse_slice(&mut read.buf_mut()[start..]);
                        return;
                    }

                    // this should only occur once at the beginning
                    log::trace!("releasing {start} initial bytes");
                    read.release(start);

                    // log::trace!("notify");
                    // empty_buffer_notifier.notify();
                }
            }
        }
    }

    /// Parse a single sync range
    fn parse_single_sync(&mut self, slice: &mut [u8]) -> Result<usize, ParseError> {
        let Some(start) = find_next_sync_point(slice) else {
            // slice did not contain any sync points
            return Err(ParseError::NoSync);
        };

        let Some(end) = find_next_sync_point(&mut slice[start + 1..]).map(|idx| idx + start + 1)
        else {
            // slice only contained a single sync point
            return Err(ParseError::OneSync(start));
        };

        log::trace!("parsing range {start}..{end}");
        self.parse_slice(&mut slice[start..end]);

        Ok(end)
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

                            // if let Some(guest_pc) = mapping.read().get(&(ip -
                            // 9)) {     writeln!
                            // (writer, "{:x}", guest_pc).unwrap();
                            // }
                        }
                        _ => (),
                    },
                    Mode::Fup => match p {
                        Packet::Fup(inner) => {
                            let _ip = inner.fup();

                            // if let Some(guest_pc) = mapping.read().get(&(ip -
                            // 9)) {     writeln!
                            // (writer, "{:x}", guest_pc).unwrap();
                            // }
                        }
                        _ => (),
                    },
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
                        panic!("{:?}", e);
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
