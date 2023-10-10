use {
    color_eyre::eyre::Result,
    std::io::{self, BufReader, BufWriter, Read, Write},
};

fn main() -> Result<()> {
    color_eyre::install()?;

    let mut stdin = BufReader::new(io::stdin());
    let mut stdout = BufWriter::new(io::stdout());

    let mut buf = [0; 8];
    while let Ok(()) = stdin.read_exact(&mut buf) {
        let pc = u64::from_le_bytes(buf);
        writeln!(stdout, "{pc:x}")?;
    }

    stdout.flush()?;

    Ok(())
}
