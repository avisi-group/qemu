use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

pub struct ThreadHandle {
    handle: JoinHandle<()>,
    shutdown_sender: Sender<()>,
}

impl ThreadHandle {
    pub fn spawn<F: FnOnce(Receiver<()>) + Send + 'static>(f: F) -> Self {
        let (shutdown_sender, shutdown_receiver) = mpsc::channel();

        Self {
            handle: thread::spawn(|| f(shutdown_receiver)),
            shutdown_sender,
        }
    }

    pub fn exit(self) {
        log::trace!("sending shutdown");
        self.shutdown_sender.send(()).unwrap();
        log::trace!("waiting on join");
        self.handle.join().unwrap();
        log::trace!("joined");
    }
}
