// https://www.johndcook.com/blog/standard_deviation/
pub struct Average {
    avg: f64,
    k: f64,
}

impl Average {
    pub fn new() -> Self {
        Self { avg: 0.0, k: 0.0 }
    }

    pub fn add(&mut self, value: impl Into<f64>) {
        let value = value.into();
        self.k += 1.0;
        self.avg += (value - self.avg) / self.k;
    }

    pub fn average(&self) -> f64 {
        self.avg
    }
}

impl<A: Into<f64>> Extend<A> for Average {
    fn extend<T: IntoIterator<Item = A>>(&mut self, iter: T) {
        iter.into_iter().for_each(|a| self.add(a))
    }
}

pub struct Variance {
    avg: Average,
    var: f64,
}

impl Variance {
    pub fn new() -> Self {
        Self {
            avg: Average::new(),
            var: 0.0,
        }
    }

    pub fn add(&mut self, value: impl Into<f64>) {
        let value = value.into();
        let left = value - self.avg.average();
        self.avg.add(value);
        let right = value - self.avg.average();
        self.var += left * right;
    }

    pub fn average(&self) -> f64 {
        self.avg.average()
    }

    pub fn variance(&self) -> f64 {
        let k = self.avg.k;
        if k <= 1.0 {
            return 0.0;
        }
        self.var / (k - 1.0)
    }

    pub fn biased_variance(&self) -> f64 {
        let k = self.avg.k;
        if k <= 0.0 {
            return 0.0;
        }
        self.var / k
    }

    pub fn std_dev(&self) -> f64 {
        self.variance().sqrt()
    }
}

impl<A: Into<f64>> Extend<A> for Variance {
    fn extend<T: IntoIterator<Item = A>>(&mut self, iter: T) {
        iter.into_iter().for_each(|a| self.add(a))
    }
}

// NOTE: both lerp functions simplify to the same expression, not sure which one is the
// best. This one is derived from my head and the other one is copied from wikipedia.
pub fn lerp(from_low: f64, from_up: f64, to_low: f64, to_up: f64, from: f64) -> f64 {
    let perc = (from - from_low) / (from_up - from_low);
    perc * (to_up - to_low) + to_low
}

pub fn lerp_u128(x0: u128, x1: u128, y0: u128, y1: u128, x: u128) -> u128 {
    assert!(x1 != x0, "can't be a vertical line");
    assert!(x >= x0, "because of unsigned ints");
    assert!(x <= x1, "because of unsigned ints");
    // NOTE: this is implied by the other asserts
    // assert!(x1 > x0, "because of unsigned ints");
    (y0 * (x1 - x) + y1 * (x - x0)) / (x1 - x0)
}

#[cfg(test)]
mod test {
    use super::*;

    fn float_cmp(a: f64, b: f64) -> bool {
        (a - b).abs() <= 0.01
    }

    #[test]
    fn test_lerp() {
        assert!(float_cmp(15.0, dbg!(lerp(1.0, 5.0, 10.0, 20.0, 3.0))));
    }

    #[test]
    fn average() {
        let mut avg = Average::new();
        assert_eq!(0.0, avg.average());

        avg.add(1);
        assert!(float_cmp(1.0, avg.average()));

        avg.add(2);
        assert!(float_cmp(1.5, avg.average()));

        avg.add(3);
        assert!(float_cmp(2.0, avg.average()));
    }

    #[test]
    fn variance() {
        let mut var = Variance::new();
        assert_eq!(0.0, var.variance());
        assert_eq!(0.0, var.average());

        var.extend(vec![1, 2, 3]);
        assert!(float_cmp(2.0, var.average()));
        assert!(float_cmp(2.0 / 2.0, var.variance()));
        assert!(float_cmp(2.0 / 3.0, var.biased_variance()));
    }
}
