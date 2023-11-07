use std::{fmt, time::Duration};

use ffmpeg::Rational;
use rkyv::{Archive, Serialize};

extern crate ffmpeg_next as ffmpeg;

#[derive(Serialize, Archive, Clone, Debug, Hash, PartialEq, Eq)]
#[archive(check_bytes)]
pub struct Timestamp {
    pub(super) timebase_numerator: i32,
    pub(super) timebase_denominator: i32,
    pub(super) timestamp: i64,
    pub(super) first_timestamp: i64,
}

impl Timestamp {
    pub(super) fn new(ts: i64, timebase: Rational, first_timestamp: i64) -> Self {
        Self {
            timestamp: ts,
            first_timestamp,
            timebase_numerator: timebase.numerator(),
            timebase_denominator: timebase.denominator(),
        }
    }

    fn parts(&self) -> (bool, f64, f64, f64, f64) {
        // TODO: Why not use ffmpeg rescale and rational if not all decimals are going to
        // be used?
        let mut total: f64 = (self.timestamp as f64 - self.first_timestamp as f64)
            * (self.timebase_numerator as f64 / self.timebase_denominator as f64);

        let negative = if total < 0.0 {
            total = -total;
            true
        } else {
            false
        };

        let subsec = (total.fract() * 1e3).trunc();
        total = total.trunc();

        let hours = (total / 3600.0).trunc();
        total %= 3600.0;

        let minutes = (total / 60.0).trunc();
        total %= 60.0;

        let seconds = total;

        (negative, hours, minutes, seconds, subsec)
    }

    pub fn duration_to_string(dur: Duration) -> String {
        Timestamp::new(
            dur.as_millis()
                .try_into()
                .expect("is probably not that big"),
            Rational::new(1, 1000),
            0,
        )
        .to_string()
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (negative, hours, minutes, seconds, subsec) = self.parts();
        let negative = negative.then_some("-").unwrap_or("");
        write!(
            f,
            "{}{:02}:{:02}:{:02}.{:03}",
            negative, hours, minutes, seconds, subsec
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn timestamp_to_string() {
        let ts = Timestamp::new(50, Rational::new(1, 1000), 0);
        assert_eq!("00:00:00.050", ts.to_string());

        let ts = Timestamp::new(1005, Rational::new(1, 1000), 0);
        assert_eq!("00:00:01.005", ts.to_string());
    }
}
