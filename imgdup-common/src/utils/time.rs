use std::time::{Duration, Instant};

pub struct Stepper {
    remains: Vec<Duration>,
    steps: Vec<Duration>,
}

impl Stepper {
    pub fn new(steps: Vec<Duration>) -> Self {
        assert!(!steps.is_empty());
        Self {
            remains: steps.clone(),
            steps,
        }
    }

    pub fn step(&mut self) -> (usize, Duration) {
        let (i, smallest) = self
            .remains
            .iter()
            .enumerate()
            .min_by_key(|(_, x)| *x)
            .map(|(i, &x)| (i, x))
            .expect("the list is non-empty");

        self.remains.iter_mut().for_each(|dur| *dur -= smallest);
        self.remains[i] = self.steps[i];

        (i, smallest)
    }

    pub fn step_non_zero(&mut self) -> (usize, Duration) {
        loop {
            let (i, step) = self.step();
            if !step.is_zero() {
                break (i, step);
            }
        }
    }
}

pub struct Every {
    every: Duration,
    last: Instant,
}

impl Every {
    pub fn new(every: Duration) -> Self {
        Self {
            every,
            last: Instant::now(),
        }
    }

    pub fn perform(&mut self, f: impl FnOnce()) {
        let now = Instant::now();
        if now - self.last >= self.every {
            self.last = now;
            f()
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn stepper_single() {
        let mut s = Stepper::new(vec![Duration::from_secs(1)]);
        for _ in 0..5 {
            assert_eq!((0, Duration::from_secs(1)), s.step());
        }
    }

    #[test]
    fn stepper_one_and_two() {
        let mut s = Stepper::new(vec![Duration::from_secs(1), Duration::from_secs(2)]);
        for _ in 0..5 {
            assert_eq!((0, Duration::from_secs(1)), s.step());
            assert_eq!((0, Duration::from_secs(1)), s.step());
            assert_eq!((1, Duration::from_secs(0)), s.step());
        }
    }

    #[test]
    fn stepper_two_and_three() {
        let mut s = Stepper::new(vec![Duration::from_secs(2), Duration::from_secs(3)]);
        for _ in 0..5 {
            assert_eq!((0, Duration::from_secs(2)), s.step());
            assert_eq!((1, Duration::from_secs(1)), s.step());
            assert_eq!((0, Duration::from_secs(1)), s.step());
            assert_eq!((0, Duration::from_secs(2)), s.step());
            assert_eq!((1, Duration::from_secs(0)), s.step());
        }
    }
}
