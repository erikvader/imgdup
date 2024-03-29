use std::{fmt, time::Duration};

use ffmpeg::{Rational, Rescale};
use rkyv::{Archive, Serialize};

extern crate ffmpeg_next as ffmpeg;

#[derive(
    Serialize,
    Archive,
    Clone,
    Debug,
    Hash,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
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

    pub(super) fn new_abs(ts: i64, timebase: Rational) -> Self {
        Self::new(ts, timebase, 0)
    }

    pub fn from_duration(dur: Duration) -> Self {
        let millis: i64 = dur
            .as_millis()
            .try_into()
            .expect("will probably not be that big");
        let to_seconds = Rational::new(1, 1000);
        Self::new_abs(millis, to_seconds)
    }

    pub fn to_duration(&self) -> Duration {
        let to_seconds = Rational::new(1, 1000);
        // NOTE: timestamp * timebase / to_seconds
        let millis = std::cmp::max(0, self.timestamp(to_seconds));
        Duration::from_millis(millis.try_into().expect("probably not a problem"))
    }

    pub(super) fn timestamp(&self, target: Rational) -> i64 {
        (self.timestamp - self.first_timestamp).rescale(self.timebase(), target)
    }

    fn timebase(&self) -> Rational {
        Rational::new(self.timebase_numerator, self.timebase_denominator)
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

impl ArchivedTimestamp {
    // TODO: figure out of rkyv deserialize works and use that instead
    pub fn deserialize(&self) -> Timestamp {
        Timestamp {
            timebase_numerator: self.timebase_numerator,
            timebase_denominator: self.timebase_denominator,
            timestamp: self.timestamp,
            first_timestamp: self.first_timestamp,
        }
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

    #[test]
    fn timestamp_duration() {
        let stamp = Timestamp::new(600, Rational::new(1, 500), 100);
        assert_eq!(Duration::from_secs(1), stamp.to_duration());

        let stamp = Timestamp::from_duration(Duration::from_secs(2));
        let ts = stamp.timestamp(Rational::new(1, 500));
        assert_eq!(1000, ts);
    }
}
