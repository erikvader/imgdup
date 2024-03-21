use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialOrd, PartialEq)]
pub struct Percent64(f64);

#[derive(thiserror::Error, Debug)]
#[error("not a valid percentage")]
pub struct PercentError;

impl Percent64 {
    pub const ZERO: Self = Percent64(0.0);

    pub fn new(float: f64) -> Result<Self, PercentError> {
        if float.is_finite() && float >= 0.0 {
            Ok(Percent64(float))
        } else {
            Err(PercentError)
        }
    }

    pub fn as_f64(self) -> f64 {
        self.0
    }

    pub fn of(part: f64, total: f64) -> Result<Self, PercentError> {
        Self::new(100.0 * part / total)
    }
}

impl From<Percent64> for f64 {
    fn from(value: Percent64) -> Self {
        value.as_f64()
    }
}

impl TryFrom<f64> for Percent64 {
    type Error = PercentError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl FromStr for Percent64 {
    type Err = PercentError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_suffix('%').ok_or(PercentError)?;
        let num: f64 = s.parse().map_err(|_| PercentError)?;
        Self::new(num)
    }
}

impl fmt::Display for Percent64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.0)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn divide_zero() {
        assert!(Percent64::of(5.0, 0.0).is_err());
    }
}
