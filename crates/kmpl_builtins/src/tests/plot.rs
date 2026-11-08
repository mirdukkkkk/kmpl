use crate::plot::svg;

fn count(text: &str, part: &str) -> usize {
    text.matches(part).count()
}

#[test]
fn a_line_with_labelled_axes() {
    let image = svg(&[0.0, 5.0, 10.0], &[0.0, 1.0, 0.5]).unwrap();
    assert!(image.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(image.ends_with("</svg>\n"));
    assert_eq!(count(&image, "<polyline"), 1);
    assert_eq!(count(&image, "<circle"), 0);
    assert!(image.contains(">10</text>"), "{image}");
    assert!(image.contains(">0.4</text>"), "{image}");
}

#[test]
fn tick_labels_have_no_rounding_noise() {
    let image = svg(&[0.0, 1.0], &[0.0, 0.7]).unwrap();
    assert!(image.contains(">0.6</text>"), "{image}");
    assert!(!image.contains("0000"), "{image}");
    assert!(!image.contains(">-0"), "{image}");
}

#[test]
fn not_finite_points_break_the_line() {
    let image = svg(&[0.0, 1.0, 2.0, 3.0, 4.0], &[1.0, 2.0, f64::NAN, 3.0, 4.0]).unwrap();
    assert_eq!(count(&image, "<polyline"), 2);
    assert!(!image.contains("NaN"), "{image}");

    let image = svg(&[0.0, 1.0, 2.0], &[1.0, f64::INFINITY, 3.0]).unwrap();
    assert_eq!(count(&image, "<circle"), 2);
    assert_eq!(count(&image, "<polyline"), 0);
    assert!(!image.contains("inf"), "{image}");
}

#[test]
fn equal_values_still_draw() {
    for value in [0.0, 5.0, -1e-300, 1e300] {
        let image = svg(&[1.0, 2.0], &[value, value]).unwrap();
        assert_eq!(count(&image, "<polyline"), 1);
        assert!(!image.contains("NaN"), "{value}: {image}");
    }
    let image = svg(&[3.0], &[4.0]).unwrap();
    assert_eq!(count(&image, "<circle"), 1);
}

#[test]
fn very_large_and_very_small_ranges() {
    let image = svg(&[0.0, 1e9], &[0.0, 1e-9]).unwrap();
    assert!(image.contains("e8</text>"), "{image}");
    assert!(image.contains("e-10</text>"), "{image}");

    assert_eq!(
        svg(&[0.0, 1.0], &[-1e308, 1e308]),
        Err("the values are too large to plot".to_owned())
    );
    assert_eq!(
        svg(&[0.0, 1.0], &[f64::MAX * 0.5, f64::MAX]),
        Err("the values are too large to plot".to_owned())
    );
}

#[test]
fn bad_input() {
    assert_eq!(
        svg(&[1.0, 2.0], &[1.0]),
        Err("`plot` needs as many x values as y values, got 2 and 1".to_owned())
    );
    assert_eq!(
        svg(&[], &[]),
        Err("`plot` needs at least one point".to_owned())
    );
    assert_eq!(
        svg(&[f64::NAN], &[1.0]),
        Err("`plot` needs at least one point with finite coordinates".to_owned())
    );
}
