pub mod any_source;
pub mod string_source;

/// A `BKTree` can be opened with this type, but not necessarily stored in it.
pub trait PartialSource {
    /// The identifier of this source, `None` if this source does not have an identifier,
    /// which means that it is not versioned or meant to be stored.
    // TODO: how to make sure only `AnySource` is returning None?
    fn identifier() -> Option<&'static str>;
}

/// This source can be stored.
pub trait Source: PartialSource {}

impl PartialSource for () {
    fn identifier() -> Option<&'static str> {
        Some("unit:1")
    }
}
impl Source for () {}
