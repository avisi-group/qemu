use scribe::hardware::decoder::sync::{
    find_next_sync, find_next_sync_ipt, find_next_sync_simple, find_next_sync_unsafe,
};

fn main() {
    divan::main();
}

fn harness<F: Fn(&[u8]) -> Option<usize>>(f: F, mut data: &[u8]) {
    while let Some(next) = f(divan::black_box(data)) {
        data = &data[next + 16..];
    }
}

#[divan::bench]
fn sync_ipt() {
    harness(find_next_sync_ipt, include_bytes!("data.pt"));
}

#[divan::bench]
fn sync_simple() {
    harness(find_next_sync_simple, include_bytes!("data.pt"));
}

#[divan::bench]
fn sync_iter() {
    harness(find_next_sync, include_bytes!("data.pt"));
}

#[divan::bench]
fn sync_unsafe() {
    harness(find_next_sync_unsafe, include_bytes!("data.pt"));
}
