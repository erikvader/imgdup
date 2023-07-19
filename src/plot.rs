use error_stack::{bail, report, IntoReport, ResultExt};
use plotters::prelude::*;
use std::path::Path;

use crate::error_stack_utils::IntoReportChangeContext;

#[derive(thiserror::Error, Debug)]
#[error("Generic plot error")]
pub struct PlotError;

pub type SvgResult<T> = error_stack::Result<T, PlotError>;

pub fn bar_chart(path: impl AsRef<Path>, bars: &[(&str, i32)]) -> SvgResult<()> {
    assert!(!bars.is_empty());
    let root = SVGBackend::new(&path, (800, 600)).into_drawing_area();
    root.fill(&WHITE).into_context(PlotError)?;

    let mut chart = ChartBuilder::on(&root)
        .caption("This is my first plot", ("sans-serif", 20).into_font())
        .build_cartesian_2d((1..bars.len()).into_segmented(), 0..9)
        .into_context(PlotError)?;

    chart.configure_mesh().draw().into_context(PlotError)?;

    chart
        .draw_series(
            Histogram::vertical(&chart)
                .style(BLUE.filled())
                .margin(10)
                .data(bars.iter().enumerate().map(|(i, (_, h))| (i, *h))),
        )
        .into_context(PlotError)?;

    root.present().into_context(PlotError)?;
    Ok(())
}

// TODO:
// pub fn perf_line(path, series: &TimeSeries)
// pub fn perf_time(path, series: &[TimeSeries])
