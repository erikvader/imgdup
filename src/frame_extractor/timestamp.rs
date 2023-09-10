use std::time::Duration;

use ffmpeg::Rational;

extern crate ffmpeg_next as ffmpeg;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
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

    pub fn to_string(&self) -> String {
        // TODO: Why not use ffmpeg rescale and rational if not all decimals are going to
        // be used?
        let mut total: f64 = (self.timestamp as f64 - self.first_timestamp as f64)
            * (self.timebase_numerator as f64 / self.timebase_denominator as f64);

        let negative = if total < 0.0 {
            total = -total;
            "-"
        } else {
            ""
        };

        let subsec = (total.fract() * 1e3).trunc();
        total = total.trunc();

        let hours = (total / 3600.0).trunc();
        total %= 3600.0;

        let minutes = (total / 60.0).trunc();
        total %= 60.0;

        let seconds = total;

        format!(
            "{}{:02}:{:02}:{:02}.{:03}",
            negative, hours, minutes, seconds, subsec
        )
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
