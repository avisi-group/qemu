use {
    color_eyre::eyre::Result,
    std::{
        env::args,
        fs::File,
        io::{BufWriter, Write},
        mem::size_of,
        path::PathBuf,
        process::exit,
    },
};

fn main() -> Result<()> {
    color_eyre::install()?;

    let Some(path) = args().nth(1) else {
        println!("missing path to trace file");
        exit(0);
    };

    let input_path = PathBuf::from(path);
    let output_path = input_path.with_extension("hex");

    let input = unsafe { memmap2::MmapOptions::new().map(&File::open(input_path)?) }?;
    let mut output = BufWriter::new(File::create(output_path)?);

    for chunk in input.chunks_exact(size_of::<u64>()) {
        let mut buf = [0; 8];
        buf.copy_from_slice(chunk);
        let pc = u64::from_le_bytes(buf);
        writeln!(output, "{pc:x}")?;
    }

    output.flush()?;

    Ok(())
}
