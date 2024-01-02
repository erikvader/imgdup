pub struct AnySource {
    _seal: (),
}

impl super::PartialSource for AnySource {
    fn identifier() -> Option<&'static str> {
        None
    }
}
