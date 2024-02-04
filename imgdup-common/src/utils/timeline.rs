use crate::utils::math::lerp_u128;
use std::time::Duration;

pub type X = Duration;
pub type Y = Duration;

pub struct Timeline {
    points: Vec<Point>,
}

struct Point {
    x: X,
    y: Y,
    curve: Curve,
}

enum Curve {
    Flat,
    Linear,
}

impl Timeline {
    pub fn new(origin_x: X, origin_y: Y) -> Self {
        Self {
            points: vec![Point {
                x: origin_x,
                y: origin_y,
                curve: Curve::Linear,
            }],
        }
    }

    pub fn new_zero() -> Self {
        Self::new(X::ZERO, Y::ZERO)
    }

    pub fn sample(&self, x: X) -> Y {
        {
            let first = self.points.first().expect("is non-empty");
            if x <= first.x {
                return first.y;
            }
        }

        let Some(next) = self.points.iter().position(|p| p.x >= x) else {
            return self.points.last().expect("is non-empty").y;
        };
        assert!(next >= 1, "x is strictly larger than the first one");
        let prev = next - 1;

        let next = self.points.get(next).expect("exists");
        let prev = self.points.get(prev).expect("exists");

        match next.curve {
            Curve::Flat => next.y,
            Curve::Linear => {
                let nanos = lerp_u128(
                    prev.x.as_nanos(),
                    next.x.as_nanos(),
                    prev.y.as_nanos(),
                    next.y.as_nanos(),
                    x.as_nanos(),
                );
                const ONE_SEC: u128 = 1_000_000_000;
                // NOTE: rust wth...
                Duration::new((nanos / ONE_SEC) as u64, (nanos % ONE_SEC) as u32)
            }
        }
    }

    pub fn add_flat(&mut self, x: X, y: Y) -> Result<(), ()> {
        self.add(Point {
            x,
            y,
            curve: Curve::Flat,
        })
    }

    pub fn add_linear(&mut self, x: X, y: Y) -> Result<(), ()> {
        self.add(Point {
            x,
            y,
            curve: Curve::Linear,
        })
    }

    // TODO: error type
    fn add(&mut self, p: Point) -> Result<(), ()> {
        if p.x <= self.points.last().expect("is non-empty").x {
            return Err(());
        }

        self.points.push(p);
        Ok(())
    }
}

#[macro_export]
macro_rules! duration {
    ($hour:literal H) => {
        Duration::from_secs(3600 * $hour)
    };
    ($min:literal M) => {
        Duration::from_secs(60 * $min)
    };
    ($sec:literal S) => {
        Duration::from_secs($sec)
    };
    ($milli:literal MS) => {
        Duration::from_millis($milli)
    };
    ($micros:literal US) => {
        Duration::from_micros($micros)
    };
    ($nanos:literal NS) => {
        Duration::from_nanos($nanos)
    };
    ($time:literal $unit:ident , $($rest:tt)*) => {
        $crate::duration!($time $unit) + $crate::duration!($($rest)*)
    }
}
pub use duration;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn duration_test() {
        assert_eq!(3600, duration!(1 H).as_secs());
        assert_eq!(3661, duration!(1 H, 1 M, 1 S).as_secs());
    }

    #[test]
    fn lerp_test() {
        assert_eq!(5, lerp_u128(0, 2, 0, 10, 1));
        assert_eq!(0, lerp_u128(0, 2, 0, 10, 0));
        assert_eq!(10, lerp_u128(0, 2, 0, 10, 2));
    }

    #[test]
    fn adding() {
        let mut line = Timeline::new_zero();
        assert!(line.add_linear(duration!(0 S), Y::ZERO).is_err());
        assert!(line.add_linear(duration!(10 S), Y::ZERO).is_ok());
        assert!(line.add_linear(duration!(10 S), Y::ZERO).is_err());
        assert!(line.add_linear(duration!(5 S), Y::ZERO).is_err());
    }

    #[test]
    fn empty() {
        let line = Timeline::new_zero();
        assert_eq!(Y::ZERO, line.sample(X::ZERO));
        assert_eq!(Y::ZERO, line.sample(duration!(1 S)));
    }

    #[test]
    fn flat() {
        let line = Timeline::new(duration!(5 S), duration!(1 M));
        assert_eq!(duration!(1 M), line.sample(X::ZERO));
        assert_eq!(duration!(1 M), line.sample(duration!(5 S)));
        assert_eq!(duration!(1 M), line.sample(duration!(10 S)));
    }

    #[test]
    fn linear() {
        let mut line = Timeline::new_zero();
        line.add_linear(duration!(1 S), duration!(4 S)).unwrap();

        assert_eq!(duration!(0 M), line.sample(X::ZERO));
        assert_eq!(duration!(2 S), line.sample(duration!(500 MS)));
        assert_eq!(duration!(4 S), line.sample(duration!(1 S)));
        assert_eq!(duration!(4 S), line.sample(duration!(2 S)));
    }

    #[test]
    fn tooth() {
        let mut line = Timeline::new_zero();
        line.add_linear(duration!(1 S), duration!(4 S)).unwrap();
        line.add_linear(duration!(2 S), duration!(0 S)).unwrap();

        assert_eq!(duration!(0 M), line.sample(X::ZERO));
        assert_eq!(duration!(2 S), line.sample(duration!(500 MS)));
        assert_eq!(duration!(4 S), line.sample(duration!(1 S)));
        assert_eq!(duration!(2 S), line.sample(duration!(1 S, 500 MS)));
        assert_eq!(duration!(0 S), line.sample(duration!(2 S)));
        assert_eq!(duration!(0 S), line.sample(duration!(3 S)));
    }

    #[test]
    fn mix() {
        let mut line = Timeline::new_zero();
        line.add_linear(duration!(1 S), duration!(4 S)).unwrap();
        line.add_flat(duration!(2 S), duration!(40 S)).unwrap();
        line.add_flat(duration!(2500 MS), duration!(60 S)).unwrap();
        line.add_linear(duration!(3 S), duration!(0 S)).unwrap();

        assert_eq!(duration!(0 M), line.sample(X::ZERO));
        assert_eq!(duration!(2 S), line.sample(duration!(500 MS)));
        assert_eq!(duration!(4 S), line.sample(duration!(1 S)));
        assert_eq!(duration!(40 S), line.sample(duration!(1500 MS)));
        assert_eq!(duration!(40 S), line.sample(duration!(2 S)));
        assert_eq!(duration!(60 S), line.sample(duration!(2200 MS)));
        assert_eq!(duration!(60 S), line.sample(duration!(2500 MS)));
        assert_eq!(duration!(30 S), line.sample(duration!(2750 MS)));
        assert_eq!(duration!(0 S), line.sample(duration!(3 S)));
        assert_eq!(duration!(0 S), line.sample(duration!(4 S)));
    }

    #[test]
    fn double_flat() {
        let mut line = Timeline::new_zero();
        line.add_flat(duration!(2 S), duration!(0 S)).unwrap();
        line.add_flat(duration!(4 S), duration!(0 S)).unwrap();

        assert_eq!(duration!(0 S), line.sample(duration!(0 S)));
        assert_eq!(duration!(0 S), line.sample(duration!(1 S)));
        assert_eq!(duration!(0 S), line.sample(duration!(2 S)));
        assert_eq!(duration!(0 S), line.sample(duration!(3 S)));
        assert_eq!(duration!(0 S), line.sample(duration!(4 S)));
        assert_eq!(duration!(0 S), line.sample(duration!(5 S)));
    }
}
