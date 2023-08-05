use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Mutex, OnceLock},
    time::{Duration, Instant},
};

pub type ID = &'static str;

pub struct Measurement {
    start: Instant,
    duration: Duration,
}

pub struct TimeSeries {
    measurements: Vec<Measurement>,
}

// https://en.wikipedia.org/wiki/Algorithms_for_calculating_variance#Welford's_online_algorithm
// http://www.johndcook.com/blog/standard_deviation/
// Räknar ut snitt, avvikelse, min och max
pub struct Stats; // TODO:

struct Perf {
    series: Mutex<HashMap<ID, TimeSeries>>,
    stats: Mutex<HashMap<ID, Stats>>, // TODO: ta bort? Vill inte ha live performance längre?
}

impl Perf {
    fn new() -> Self {
        Self {
            series: Mutex::new(HashMap::new()),
            stats: Mutex::new(HashMap::new()),
        }
    }

    fn instance() -> &'static Self {
        static PERF: OnceLock<Perf> = OnceLock::new();
        PERF.get_or_init(|| Perf::new())
    }

    fn publish(&self, id: ID, meas: Measurement) {
        self.series
            .lock()
            .unwrap()
            .entry(id)
            .or_insert_with(|| TimeSeries::new())
            .push(meas);
    }

    fn finish(&self) -> HashMap<ID, TimeSeries> {
        self.stats.lock().unwrap().clear();
        std::mem::take(&mut self.series.lock().unwrap())
    }
}

impl TimeSeries {
    fn new() -> Self {
        Self {
            measurements: Vec::with_capacity(1024),
        }
    }

    fn push(&mut self, meas: Measurement) {
        self.measurements.push(meas);
    }

    fn sort(&mut self) {
        self.measurements.sort_by_key(|meas| meas.start)
    }

    pub fn measurements(&self) -> &[Measurement] {
        &self.measurements
    }

    pub fn start(&self) -> Instant {
        self.measurements.first().expect("cannot be empty").start()
    }

    pub fn end(&self) -> Instant {
        self.measurements.last().expect("cannot be empty").end()
    }

    pub fn duration(&self) -> Duration {
        self.end() - self.start()
    }
}

impl Measurement {
    fn new(start: Instant, duration: Duration) -> Self {
        Self { start, duration }
    }

    pub fn start(&self) -> Instant {
        self.start
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }

    pub fn end(&self) -> Instant {
        self.start + self.duration
    }
}

static PERF_ENABLE: AtomicBool = AtomicBool::new(false);

pub fn enable(enable: bool) {
    PERF_ENABLE.store(enable, std::sync::atomic::Ordering::SeqCst);
}

pub fn is_enabled() -> bool {
    PERF_ENABLE.load(std::sync::atomic::Ordering::SeqCst)
}

pub struct Cookie {
    instant: Option<Instant>,
}

pub fn start() -> Cookie {
    Cookie {
        instant: is_enabled().then(|| Instant::now()),
    }
}

pub fn end(id: ID, cookie: Cookie) {
    match cookie {
        Cookie {
            instant: Some(earlier),
        } => {
            let dur = earlier.elapsed();
            Perf::instance().publish(id, Measurement::new(earlier, dur));
        }
        _ => (),
    }
}

// TODO:
// pub fn subscribe(id: ID) -> Receiver<Stats> {}
// Probably a much better idea:
// pub fn subscribe(id: ID);
// pub fn stats(id: ID) -> Option<Stats> {}

pub fn finish() -> HashMap<ID, TimeSeries> {
    enable(false);
    let mut series = Perf::instance().finish();
    series.values_mut().for_each(|v| v.sort());
    series
}

// TODO:
//pub fn save(&[TimeSeries])
//pub fn save() -> Vec<TimeSeries>

#[macro_export]
macro_rules! perf {
    ($id:expr, $expr:expr) => {{
        let cookie = $crate::perf::start();
        let retval = $expr;
        $crate::perf::end($id, cookie);
        retval
    }};
}
