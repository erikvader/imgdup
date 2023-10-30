pub mod any_source;
pub mod video_source;

mod private {
    pub trait Seal {}
}

/// A `BKTree` can be opened with this type, but not necessarily stored in it.
pub trait PartialSource: private::Seal {
    /// The identifier of this source, `None` if this source is not meant to be stored, i.e., is the special type `AnySource`.
    fn identifier() -> Option<&'static str>;
}

/// This source can be stored.
pub trait Source: PartialSource {
    // TODO: is it possible to staticly assert that `identifier` is non-none?
}
