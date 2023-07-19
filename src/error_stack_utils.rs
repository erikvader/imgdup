use error_stack::{IntoReport, ResultExt};

/// Just a shorthand for writing .into_report().change_context(...)
pub trait IntoReportChangeContext {
    type Ok;
    fn into_context<C: error_stack::Context>(
        self,
        context: C,
    ) -> error_stack::Result<Self::Ok, C>;
}

impl<R> IntoReportChangeContext for R
where
    R: IntoReport,
{
    type Ok = <R as IntoReport>::Ok;

    fn into_context<C: error_stack::Context>(
        self,
        context: C,
    ) -> error_stack::Result<Self::Ok, C> {
        self.into_report().change_context(context)
    }
}
