use {
    color_eyre::eyre::Result,
    memmap2::Mmap,
    parking_lot::RwLock,
    scribe::hardware::{
        notify::Notify, ordered_queue, reader::TaskManager, writer::Writer, TipParser, TipWriter,
    },
    std::{
        env::args,
        fs::File,
        path::{Path, PathBuf},
        process::exit,
        sync::{atomic::AtomicU32, Arc},
    },
};

fn main() -> Result<()> {
    color_eyre::install()?;
    pretty_env_logger::formatted_timed_builder()
        .filter_level(log::LevelFilter::Trace)
        .try_init()
        .unwrap();

    let Some(arg) = args().nth(1) else {
        println!("missing path to PT data");
        exit(-1);
    };

    let path = PathBuf::from(arg);

    let raw = open_path(&path)?;
    let pc_map = Arc::new(RwLock::new(
        serde_json::from_reader(File::open("/tmp/pt/pcmap.json").unwrap()).unwrap(),
    ));

    let (sender, receiver) = ordered_queue::new();
    let empty_buffer_notifier = Notify::new();
    let task_count = Arc::new(AtomicU32::new(0));

    let writer = Writer::init::<TipWriter, _>(
        path.parent().unwrap().join("tip.trace"),
        pc_map,
        receiver,
        empty_buffer_notifier.clone(),
        task_count.clone(),
    );

    let mut task_manager = TaskManager::<TipParser>::new(sender, task_count);
    let mut current_index = 0;

    loop {
        let consumed = task_manager.callback(false)(&raw[current_index..]);

        if consumed == 0 {
            break;
        } else {
            current_index += consumed;
        }
    }
    let consumed = task_manager.callback(true)(&raw[current_index..]);
    assert_eq!(current_index + consumed, raw.len());

    log::trace!("sending exit");
    writer.exit();

    Ok(())
}

fn open_path<P: AsRef<Path>>(path: P) -> Result<Mmap> {
    Ok(unsafe { memmap2::MmapOptions::new().map(&File::open(path)?) }?)
}
