pub mod any_source;
pub mod string_source;
pub mod video_source;

mod private {
    pub trait Seal {}
}

/// A `BKTree` can be opened with this type, but not necessarily stored in it.
pub trait PartialSource: private::Seal {
    /// The identifier of this source, `None` if this source does not have an identifier,
    /// which means that it is not versioned or meant to be stored.
    fn identifier() -> Option<&'static str>;
}

/// This source can be stored.
pub trait Source: PartialSource {}

impl private::Seal for () {}
impl PartialSource for () {
    fn identifier() -> Option<&'static str> {
        Some("unit:1")
    }
}
impl Source for () {}
