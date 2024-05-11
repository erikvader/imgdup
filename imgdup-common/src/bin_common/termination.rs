use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use signal_hook::{consts::signal::*, low_level};

#[derive(Clone, Debug)]
pub struct Cookie {
    count: Arc<AtomicUsize>,
}

impl Cookie {
    pub fn new() -> Result<Self, std::io::Error> {
        let count = Arc::new(AtomicUsize::new(0));

        for flag in [SIGINT, SIGTERM] {
            let count = Arc::clone(&count);
            // SAFETY: this only uses atomic stuff and functions the crate itself is using
            // in signal handlers
            unsafe {
                low_level::register(flag, move || {
                    let prev = count.fetch_add(1, Ordering::SeqCst);
                    if prev >= 2 {
                        let _ = low_level::emulate_default_handler(flag);
                    }
                })?;
            };
        }

        Ok(Self { count })
    }

    pub fn is_terminating(&self) -> bool {
        self.count.load(Ordering::SeqCst) >= 1
    }

    pub fn is_terminating_hard(&self) -> bool {
        self.count.load(Ordering::SeqCst) >= 2
    }
}
