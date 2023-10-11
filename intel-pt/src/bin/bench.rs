use scribe::intel_pt::{notify::Notify, BUFFER_SIZE};
use {
    bbqueue::BBBuffer,
    parking_lot::RwLock,
    scribe::{
        intel_pt::{parser::Parser, writer::Writer},
        Mode,
    },
    std::{collections::HashMap, sync::Arc},
};

const DATA: &[u8] = include_bytes!("/home/fm208/data/ringbuffer_ptwrite.dump");

static BUFFER: BBBuffer<BUFFER_SIZE> = BBBuffer::new();

fn main() {
    pretty_env_logger::formatted_timed_builder()
        .filter_level(log::LevelFilter::Trace)
        .try_init()
        .unwrap();

    let pc_map = Arc::new(RwLock::new(HashMap::default()));
    let (mut producer, consumer) = BUFFER.try_split().unwrap();
    let empty_buffer_notifier = Notify::new();

    {
        let mut wgr = producer.grant_exact(DATA.len()).unwrap();
        wgr.buf().copy_from_slice(DATA);
        wgr.commit(DATA.len());
    }

    let (writer, queue) = Writer::init("/home/fm208/data/intelpt.trace", pc_map, Mode::PtWrite);
    let parser = Parser::init(empty_buffer_notifier, consumer, queue, Mode::PtWrite);

    std::thread::sleep(std::time::Duration::from_millis(100));

    parser.exit();
    writer.exit();
}
