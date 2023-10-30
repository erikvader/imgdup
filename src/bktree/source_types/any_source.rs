pub struct AnySource {
    _seal: (),
}

impl super::private::Seal for AnySource {}
impl super::PartialSource for AnySource {
    fn identifier() -> Option<&'static str> {
        None
    }
}