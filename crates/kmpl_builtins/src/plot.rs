//! `plot(xs, ys, path)`: a line chart written to an SVG file.
//!
//! SVG is plain text, so this needs no libraries. Points with a `NaN` or
//! infinite coordinate are left out and break the line. The axes fit the
//! remaining points.

use crate::Value;

const WIDTH: f64 = 800.0;
const HEIGHT: f64 = 500.0;
const LEFT: f64 = 80.0;
const RIGHT: f64 = 24.0;
const TOP: f64 = 24.0;
const BOTTOM: f64 = 48.0;

const GRID_COLOR: &str = "#e3e6ea";
const FRAME_COLOR: &str = "#8a939e";
const LABEL_COLOR: &str = "#333a42";
const LINE_COLOR: &str = "#2563eb";

/// Space left around the points, as a part of their range.
const PADDING: f64 = 0.03;
/// Below this a range cannot be divided into ticks: powers of ten underflow.
const SMALLEST_RANGE: f64 = 1e-290;

pub fn plot(xs: &[Value], ys: &[Value], path: &str) -> Result<(), String> {
    let xs: Vec<f64> = xs.iter().map(Value::as_real).collect();
    let ys: Vec<f64> = ys.iter().map(Value::as_real).collect();
    let image = svg(&xs, &ys)?;
    std::fs::write(path, image).map_err(|e| format!("cannot write `{path}`: {e}"))
}

/// One axis: the range it shows and its labelled positions.
struct Axis {
    low: f64,
    high: f64,
    ticks: Vec<(f64, String)>,
}

impl Axis {
    fn new(values: impl Iterator<Item = f64>) -> Result<Axis, String> {
        let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
        for v in values {
            low = low.min(v);
            high = high.max(v);
        }

        let range = high - low;
        if !range.is_finite() {
            return Err("the values are too large to plot".to_owned());
        }

        if range < SMALLEST_RANGE || range <= high.abs().max(low.abs()) * 1e-12 {
            // All values are equal, or as good as: show them mid-axis.
            let pad = if low.abs() < SMALLEST_RANGE {
                1.0
            } else {
                low.abs() * 0.5
            };
            low -= pad;
            high += pad;
        } else {
            low -= range * PADDING;
            high += range * PADDING;
        }
        if !low.is_finite() || !high.is_finite() {
            return Err("the values are too large to plot".to_owned());
        }

        Ok(Axis {
            low,
            high,
            ticks: ticks(low, high),
        })
    }

    /// Where `v` lies on this axis, from 0 to 1.
    fn position(&self, v: f64) -> f64 {
        (v - self.low) / (self.high - self.low)
    }
}

/// Round values between `low` and `high`, four to nine of them.
fn ticks(low: f64, high: f64) -> Vec<(f64, String)> {
    let step = tick_step((high - low) / 8.0);
    let first = (low / step).ceil();
    let count = ((high / step).floor() - first).clamp(0.0, 20.0) as usize;

    (0..=count)
        .map(|i| {
            // `+ 0.0` turns `-0` into `0`.
            let value = (first + i as f64) * step + 0.0;
            (value, label(value, step))
        })
        .filter(|(value, _)| (low..=high).contains(value))
        .collect()
}

/// The first of 1, 2, 5 or 10 times a power of ten that is at least `raw`.
fn tick_step(raw: f64) -> f64 {
    let magnitude = 10f64.powf(raw.log10().floor());
    let fraction = raw / magnitude;
    let nice = if fraction <= 1.0 {
        1.0
    } else if fraction <= 2.0 {
        2.0
    } else if fraction <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * magnitude
}

/// With only as many decimals as the step needs: `0.2`, not
/// `0.20000000000000001`.
fn label(value: f64, step: f64) -> String {
    if !(1e-4..1e6).contains(&step) {
        return format!("{value:.2e}");
    }
    let decimals = (-step.log10().floor()).max(0.0) as usize;
    format!("{value:.decimals$}")
}

pub(crate) fn svg(xs: &[f64], ys: &[f64]) -> Result<String, String> {
    if xs.len() != ys.len() {
        return Err(format!(
            "`plot` needs as many x values as y values, got {} and {}",
            xs.len(),
            ys.len()
        ));
    }
    if xs.is_empty() {
        return Err("`plot` needs at least one point".to_owned());
    }

    let points: Vec<(f64, f64)> = xs.iter().copied().zip(ys.iter().copied()).collect();
    let is_finite = |&(x, y): &(f64, f64)| x.is_finite() && y.is_finite();
    if !points.iter().any(is_finite) {
        return Err("`plot` needs at least one point with finite coordinates".to_owned());
    }

    let x_axis = Axis::new(points.iter().filter(|p| is_finite(p)).map(|p| p.0))?;
    let y_axis = Axis::new(points.iter().filter(|p| is_finite(p)).map(|p| p.1))?;

    let (plot_width, plot_height) = (WIDTH - LEFT - RIGHT, HEIGHT - TOP - BOTTOM);
    let (right, bottom) = (LEFT + plot_width, TOP + plot_height);
    let to_x = |x: f64| LEFT + x_axis.position(x) * plot_width;
    let to_y = |y: f64| bottom - y_axis.position(y) * plot_height;

    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{HEIGHT}\" \
         viewBox=\"0 0 {WIDTH} {HEIGHT}\" font-family=\"sans-serif\" font-size=\"13\">\n\
         <rect width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"white\"/>\n"
    );

    for (value, text) in &x_axis.ticks {
        let x = to_x(*value);
        out.push_str(&format!(
            "<line x1=\"{x:.2}\" y1=\"{TOP}\" x2=\"{x:.2}\" y2=\"{bottom}\" stroke=\"{GRID_COLOR}\"/>\n\
             <text x=\"{x:.2}\" y=\"{}\" text-anchor=\"middle\" fill=\"{LABEL_COLOR}\">{text}</text>\n",
            bottom + 22.0
        ));
    }
    for (value, text) in &y_axis.ticks {
        let y = to_y(*value);
        out.push_str(&format!(
            "<line x1=\"{LEFT}\" y1=\"{y:.2}\" x2=\"{right}\" y2=\"{y:.2}\" stroke=\"{GRID_COLOR}\"/>\n\
             <text x=\"{}\" y=\"{:.2}\" text-anchor=\"end\" fill=\"{LABEL_COLOR}\">{text}</text>\n",
            LEFT - 10.0,
            y + 4.0
        ));
    }
    out.push_str(&format!(
        "<rect x=\"{LEFT}\" y=\"{TOP}\" width=\"{plot_width}\" height=\"{plot_height}\" \
         fill=\"none\" stroke=\"{FRAME_COLOR}\"/>\n"
    ));

    for segment in points.split(|p| !is_finite(p)) {
        match segment {
            [] => {}
            // A line through one point would be invisible.
            [(x, y)] => out.push_str(&format!(
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"3\" fill=\"{LINE_COLOR}\"/>\n",
                to_x(*x),
                to_y(*y)
            )),
            _ => {
                let coordinates: Vec<String> = segment
                    .iter()
                    .map(|(x, y)| format!("{:.2},{:.2}", to_x(*x), to_y(*y)))
                    .collect();
                out.push_str(&format!(
                    "<polyline fill=\"none\" stroke=\"{LINE_COLOR}\" stroke-width=\"2\" \
                     stroke-linejoin=\"round\" points=\"{}\"/>\n",
                    coordinates.join(" ")
                ));
            }
        }
    }

    out.push_str("</svg>\n");
    Ok(out)
}
