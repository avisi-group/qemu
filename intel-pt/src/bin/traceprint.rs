use {
    color_eyre::eyre::Result,
    std::io::{self, BufReader, BufWriter, Read, Write},
};

const BUFFER_SIZE: usize = 8 * 1024;

fn main() -> Result<()> {
    color_eyre::install()?;

    let mut stdin = BufReader::with_capacity(BUFFER_SIZE, io::stdin());
    let mut stdout = BufWriter::with_capacity(BUFFER_SIZE, io::stdout());

    let mut buf = [0; 8];
    while let Ok(()) = stdin.read_exact(&mut buf) {
        let pc = u64::from_le_bytes(buf);
        writeln!(stdout, "{pc:x}").unwrap();
    }

    Ok(())
}
