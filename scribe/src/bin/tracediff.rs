use {
    color_eyre::eyre::Result,
    memmap2::Mmap,
    std::{env::args, fs::File, mem::size_of, path::Path, process::exit},
};

fn main() -> Result<()> {
    color_eyre::install()?;

    let Some(left) = args().nth(1) else {
        println!("missing path to first trace file");
        exit(-1);
    };

    let Some(right) = args().nth(2) else {
        println!("missing path to second trace file");
        exit(-1);
    };

    let left = open_path(left)?;
    let right = open_path(right)?;

    left.chunks_exact(size_of::<u64>())
        .zip(right.chunks_exact(size_of::<u64>()))
        .enumerate()
        .for_each(|(idx, (l, r))| {
            if l != r {
                let l = {
                    let mut buf = [0; 8];
                    buf.copy_from_slice(l);
                    u64::from_le_bytes(buf)
                };
                let r = {
                    let mut buf = [0; 8];
                    buf.copy_from_slice(r);
                    u64::from_le_bytes(buf)
                };
                println!("{idx}: {l:x} != {r:x}");
                exit(-1);
            }
        });

    Ok(())
}

fn open_path<P: AsRef<Path>>(path: P) -> Result<Mmap> {
    Ok(unsafe { memmap2::MmapOptions::new().map(&File::open(path)?) }?)
}
