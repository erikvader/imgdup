use color_eyre::eyre;
use plotters::prelude::*;
use std::path::Path;

pub fn bar_chart<X, P>(path: P, bars: &[(X, u32)]) -> eyre::Result<()>
where
    P: AsRef<Path>,
    X: ToString,
{
    assert!(!bars.is_empty());
    let width = 50 + 100 * bars.len();
    let max_val = bars.iter().map(|(_, val)| *val).max().unwrap();

    let root = SVGBackend::new(&path, (width as u32, 800)).into_drawing_area();
    root.fill(&WHITE)?;

    let mut chart = ChartBuilder::on(&root)
        // .caption("This is my first plot", ("sans-serif", 20).into_font())
        .margin(5)
        .set_left_and_bottom_label_area_size(20)
        .build_cartesian_2d((1..bars.len()).into_segmented(), (0..max_val).log_scale())?;

    chart
        .configure_mesh()
        .x_label_formatter(&|i: &SegmentValue<usize>| match i {
            SegmentValue::Exact(_) => "exact??".to_string(),
            SegmentValue::CenterOf(i) => bars[*i - 1].0.to_string(),
            SegmentValue::Last => "last??".to_string(),
        })
        .x_labels(bars.len())
        .draw()?;

    chart.draw_series(
        Histogram::vertical(&chart)
            .style(BLUE.filled())
            .margin(10)
            .data(bars.iter().enumerate().map(|(i, (_, h))| (i + 1, *h))),
    )?;

    root.present()?;
    Ok(())
}

// TODO:
// rita en timeseries som en linje, x: tiden när det hände, y: dur
// pub fn perf_line(path, series: &TimeSeries)
// rita allihopa som flera horisontella linjesegment där den vänstra punkten är start och
// den högra är slut
// pub fn perf_time(path, series: &[TimeSeries])
