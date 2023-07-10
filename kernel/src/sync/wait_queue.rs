use alloc::collections::VecDeque;
use spin::lock_api::Mutex;

use super::wait_condition::WaitCondition;

#[derive(Debug)]
pub struct WaitQueue<T> {
    inner: Mutex<VecDeque<T>>,
    waitcond: WaitCondition,
}

impl<T> WaitQueue<T> {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(VecDeque::new()),
            waitcond: WaitCondition::new(),
        }
    }

    pub fn send(&self, data: T) {
        let mut queue = self.inner.lock();
        queue.push_back(data);
        drop(queue);
        self.waitcond.notify_all();
    }

    pub fn receive(&self) -> T {
        loop {
            let mut queue = self.inner.lock();
            if let Some(data) = queue.pop_front() {
                return data;
            }
            drop(queue);
            self.waitcond.wait();
        }
    }
}
