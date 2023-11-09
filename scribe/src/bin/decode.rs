use {
    color_eyre::eyre::Result,
    memmap2::Mmap,
    scribe::hardware::{
        notify::Notify, ordered_queue, reader::TaskManager, writer::Writer, PtwParser, PtwWriter,
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

    let Some(arg) = args().nth(1) else {
        println!("missing path to PT data");
        exit(-1);
    };

    let path = PathBuf::from(arg);

    let raw = open_path(&path)?;

    let (sender, receiver) = ordered_queue::new();
    let empty_buffer_notifier = Notify::new();
    let task_count = Arc::new(AtomicU32::new(0));

    let writer = Writer::init::<PtwWriter, _>(
        path.parent().unwrap().join("ptw.trace"),
        (),
        receiver,
        empty_buffer_notifier.clone(),
        task_count.clone(),
    );

    let mut task_manager = TaskManager::<PtwParser>::new(sender, task_count);
    let mut current_index = 0;

    while current_index < raw.len() {
        let consumed = task_manager.callback(false)(&raw[current_index..]);
        current_index += consumed;
    }

    writer.exit();

    Ok(())
}

fn open_path<P: AsRef<Path>>(path: P) -> Result<Mmap> {
    Ok(unsafe { memmap2::MmapOptions::new().map(&File::open(path)?) }?)
}
