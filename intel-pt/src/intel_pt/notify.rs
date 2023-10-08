use parking_lot::{Condvar, Mutex};
use std::sync::Arc;

#[derive(Clone)]
pub struct Notify {
    inner: Arc<(Mutex<bool>, Condvar)>,
}

impl Notify {
    pub fn new() -> Self {
        Self {
            inner: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }

    /// Notifies any waiting threads
    pub fn notify(&self) {
        let mut fired = self.inner.0.lock();
        *fired = true;
        self.inner.1.notify_one();
    }

    /// Waits for any notifications
    pub fn wait(&self) {
        let mut fired = self.inner.0.lock();
        if !*fired {
            self.inner.1.wait(&mut fired);
        }
        *fired = false;
    }
}
