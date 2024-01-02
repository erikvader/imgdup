use std::ops::Deref;

use rkyv::{Archive, Serialize};

#[derive(Clone, Serialize, Archive, Debug)]
#[archive(check_bytes)]
pub struct StringSource(pub String);

impl super::Source for StringSource {
    fn identifier() -> &'static str {
        "string:1"
    }
}

impl Deref for StringSource {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for ArchivedStringSource {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl StringSource {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ArchivedStringSource {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
