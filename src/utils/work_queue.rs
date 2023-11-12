// TODO: remove this module?

use std::sync::atomic::AtomicUsize;

pub struct WorkQueue<T> {
    work: Vec<T>,
    next: AtomicUsize,
}

impl<T> WorkQueue<T> {
    pub fn new(work: Vec<T>) -> Self {
        Self {
            work,
            next: AtomicUsize::new(0),
        }
    }

    pub fn next(&self) -> Option<&T> {
        self.next_index().map(|(_, t)| t)
    }

    pub fn next_index(&self) -> Option<(usize, &T)> {
        let cur = self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.work.get(cur).map(|t| (cur, t))
    }

    pub fn len(&self) -> usize {
        self.work.len()
    }
}
