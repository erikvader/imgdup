use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

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
        let cur = self.next.fetch_add(1, SeqCst);
        self.work.get(cur).map(|t| (cur, t))
    }

    pub fn len(&self) -> usize {
        self.work.len()
    }

    pub fn stop(&self) {
        self.next.store(self.len(), SeqCst);
    }

    pub fn is_stopped(&self) -> bool {
        self.next.load(SeqCst) >= self.len()
    }
}
