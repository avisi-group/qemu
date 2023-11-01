use {
    bbqueue::BBBuffer,
    memmap2::Mmap,
    scribe::hardware::{
        notify::Notify, ordered_queue, parser::Parser, writer::Writer, PacketParser, PacketWriter,
        BUFFER_SIZE,
    },
    std::fs::File,
};

const RAW_DATA_PATH: &str = "/home/fm208/data/ptdump.tip";

static BUFFER: BBBuffer<BUFFER_SIZE> = BBBuffer::new();

fn main() {
    pretty_env_logger::formatted_timed_builder()
        .filter_level(log::LevelFilter::Trace)
        .try_init()
        .unwrap();

    let (mut producer, consumer) = BUFFER.try_split().unwrap();
    let empty_buffer_notifier = Notify::new();
    let ready_notifier = Notify::new();
    let (sender, receiver) = ordered_queue::new();

    let writer = Writer::init::<NoopWriter, _>("/dev/null", (), receiver, ready_notifier.clone());
    let parser = Parser::init::<PrintParser>(
        empty_buffer_notifier.clone(),
        ready_notifier,
        consumer,
        sender,
    );

    let mmap = unsafe { Mmap::map(&File::open(RAW_DATA_PATH).unwrap()) }.unwrap();

    mmap.chunks(4096).for_each(|chunk| {
        let mut wgr = producer.grant_exact(chunk.len()).unwrap();
        wgr.buf().copy_from_slice(chunk);
        wgr.commit(chunk.len());
        empty_buffer_notifier.wait();
    });

    parser.exit();
    writer.exit();
}

struct NoopWriter;

impl PacketWriter for NoopWriter {
    type ProcessedPacket = ();

    type Ctx = ();

    fn new(_: Self::Ctx) -> Self {
        Self
    }

    fn calculate_pc(&mut self, _: Self::ProcessedPacket) -> Option<u64> {
        None
    }
}

struct PrintParser;

impl PacketParser for PrintParser {
    type ProcessedPacket = ();

    fn new() -> Self {
        Self
    }

    fn process(&mut self, packet: libipt::packet::Packet<()>) {
        println!("{packet:#?}");
    }

    fn finish(self) -> Vec<Self::ProcessedPacket> {
        vec![]
    }
}
