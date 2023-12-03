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

#[cfg(test)]
mod test {
    use super::*;

    fn float_cmp(a: f64, b: f64) -> bool {
        (a - b).abs() <= 0.01
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
