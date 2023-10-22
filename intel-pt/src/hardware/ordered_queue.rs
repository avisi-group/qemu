//! Data is parsed in parallel and so potentially out of order, threads push data into this queue and the writer reads ordered data out

use parking_lot::Mutex;
use std::{collections::BinaryHeap, sync::Arc};

/// Some payload of type `T` with an associated sequence number
struct Sequenced<T> {
    sequence_number: u64,
    payload: T,
}

impl<T> PartialEq for Sequenced<T> {
    fn eq(&self, other: &Self) -> bool {
        self.sequence_number == other.sequence_number
    }
}

impl<T> Eq for Sequenced<T> {}

impl<T> PartialOrd for Sequenced<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        other.sequence_number.partial_cmp(&self.sequence_number)
    }
}

impl<T> Ord for Sequenced<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.sequence_number.cmp(&self.sequence_number)
    }
}

type Queue<T> = Arc<Mutex<BinaryHeap<Sequenced<T>>>>;

pub fn new<T: Send>() -> (Sender<T>, Receiver<T>) {
    let queue = Queue::default();
    (
        Sender {
            queue: queue.clone(),
        },
        Receiver {
            queue,
            next_sequence_number: 0,
        },
    )
}

#[derive(Clone)]
pub struct Sender<T> {
    queue: Queue<T>,
}

impl<T> Sender<T> {
    pub fn send(&self, sequence_number: u64, payload: T) {
        self.queue.lock().push(Sequenced {
            sequence_number,
            payload,
        })
    }
}

pub struct Receiver<T> {
    queue: Queue<T>,
    next_sequence_number: u64,
}

impl<T> Receiver<T> {
    pub fn recv(&mut self) -> Option<T> {
        let mut guard = self.queue.lock();

        let Some(Sequenced {
            sequence_number, ..
        }) = guard.peek()
        else {
            return None;
        };

        if *sequence_number == self.next_sequence_number {
            self.next_sequence_number += 1;
            Some(guard.pop().unwrap().payload)
        } else {
            None
        }
    }
}
