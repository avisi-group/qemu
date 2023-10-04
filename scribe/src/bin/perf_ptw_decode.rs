//"perf", "record" ,"-e" ,"intel_pt/config=0x1001/", "./bin/qemu-insert-ptw"

use {
    color_eyre::eyre::Result,
    linux_perf_data::{
        linux_perf_event_reader::RawData, PerfFileReader, PerfFileRecord, UserRecord,
    },
    scribe::hardware::{
        notify::Notify, ordered_queue, reader::TaskManager, writer::Writer, PtwParser, PtwWriter,
    },
    std::{
        env::args,
        fs::File,
        io::BufReader,
        path::PathBuf,
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

    let mut task_manager = TaskManager::<PtwParser>::new(sender, task_count.clone());

    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(reader)?;

    let mut buffer = Vec::new();

    loop {
        let next = match record_iter.next_record(&mut perf_file) {
            Ok(next) => next,
            Err(linux_perf_data::Error::InvalidPerfEventSize) => {
                continue;
            }
            Err(e) => {
                println!("error: {e:?}");
                break;
            }
        };

        let Some(record) = next else {
            break;
        };

        let PerfFileRecord::UserRecord(record) = record else {
            continue;
        };

        let UserRecord::Raw(raw) = record.parse()? else {
            continue;
        };

        // copy out raw data
        match raw.data {
            RawData::Single(slice) => {
                buffer.extend_from_slice(slice);
            }
            RawData::Split(a, b) => {
                buffer.extend_from_slice(a);
                buffer.extend_from_slice(b);
            }
        }

        let consumed = task_manager.callback(false)(&buffer);
        // probably faster to copy out any remainder idk
        buffer = buffer[consumed..].to_owned();
    }

    let _ = task_manager.callback(true)(&buffer);

    writer.exit();

    // File::create("ptw.raw").unwrap().write_all(&buffer).unwrap();

    Ok(())
}
