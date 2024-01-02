pub mod string_source;

mod private {
    pub trait Seal {}
}

/// A `BKTree` can be opened with this type, but not necessarily stored in it.
pub trait PartialSource: private::Seal {
    /// The identifier of this source, `None` if this source does not have an identifier,
    /// which means that it is not versioned or meant to be stored.
    fn partial_identifier() -> Option<&'static str>;
}

/// This source can be stored.
pub trait Source: PartialSource {
    fn identifier() -> &'static str;
}

impl<T> PartialSource for T
where
    T: Source,
{
    fn partial_identifier() -> Option<&'static str> {
        Some(Self::identifier())
    }
}

impl<T: Source> private::Seal for T {}

impl Source for () {
    fn identifier() -> &'static str {
        "unit:1"
    }
}

pub struct AnySource {
    _seal: (),
}

impl private::Seal for AnySource {}
impl PartialSource for AnySource {
    fn partial_identifier() -> Option<&'static str> {
        None
    }
}
