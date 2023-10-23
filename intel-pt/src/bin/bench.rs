// use {
//     bbqueue::BBBuffer,
//     parking_lot::RwLock,
//     scribe::{
//         hardware::{notify::Notify, ordered_queue, parser::Parser,
// writer::Writer, BUFFER_SIZE},         Mode,
//     },
//     std::{collections::HashMap, sync::Arc},
// };

//const DATA: &[u8] =
// include_bytes!("/home/fm208/data/ringbuffer_ptwrite.dump");

//static BUFFER: BBBuffer<BUFFER_SIZE> = BBBuffer::new();

fn main() {
    // pretty_env_logger::formatted_timed_builder()
    //     .filter_level(log::LevelFilter::Trace)
    //     .try_init()
    //     .unwrap();

    // let pc_map = Arc::new(RwLock::new(HashMap::default()));
    // let (mut producer, consumer) = BUFFER.try_split().unwrap();
    // let empty_buffer_notifier = Notify::new();
    // let (sender, receiver) = ordered_queue::new();

    // {
    //     let mut wgr = producer.grant_exact(DATA.len()).unwrap();
    //     wgr.buf().copy_from_slice(DATA);
    //     wgr.commit(DATA.len());
    // }

    // let writer = Writer::init(
    //     "/home/fm208/data/intelpt.trace",
    //     pc_map,
    //     receiver,
    //     Mode::PtWrite,
    // );
    // let parser = Parser::init(empty_buffer_notifier, consumer, sender,
    // Mode::PtWrite);

    // std::thread::sleep(std::time::Duration::from_millis(100));

    // parser.exit();
    // writer.exit();
}
