use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use signal_hook::{
    consts::signal::*,
    flag::{register, register_conditional_default},
};

#[derive(Clone, Debug)]
pub struct Cookie {
    terminating: Arc<AtomicBool>,
}

impl Cookie {
    pub fn new() -> Result<Self, std::io::Error> {
        let terminating = Arc::new(AtomicBool::new(false));

        for flag in [SIGINT, SIGTERM] {
            register_conditional_default(flag, terminating.clone())?;
            register(flag, terminating.clone())?;
        }

        Ok(Self { terminating })
    }

    pub fn is_terminating(&self) -> bool {
        self.terminating.load(Ordering::SeqCst)
    }
}
