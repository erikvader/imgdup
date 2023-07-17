struct MyLogger;

static MY_LOGGER: MyLogger = MyLogger;

impl log::Log for MyLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            println!(
                "{} [{}] - {}",
                record.target(),
                record.level(),
                record.args(),
            );
        }
    }

    fn flush(&self) {}
}

pub fn install() {
    log::set_logger(&MY_LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
}
