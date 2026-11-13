//! Display units. The model is always meters; this only changes how lengths
//! are shown and typed: m, cm, mm, ft (feet and inches) or in.

/// Format a length in meters for display in `units`.
pub fn format(m: f64, units: &str) -> String {
    match units {
        "cm" => format!("{:.1} cm", m * 100.0),
        "mm" => format!("{:.0} mm", m * 1000.0),
        "ft" => feet_inches(m),
        "in" => format!("{}\"", inches_frac(m / 0.0254)),
        _ => format!("{m:.2} m"),
    }
}

/// 3.5052 m -> 11' 6"
fn feet_inches(m: f64) -> String {
    let total = (m.abs() / 0.0254 * 8.0).round() / 8.0; // nearest 1/8"
    let feet = (total / 12.0).floor();
    let inches = total - feet * 12.0;
    let sign = if m < 0.0 { "-" } else { "" };
    if feet == 0.0 {
        format!("{sign}{}\"", inches_frac(inches))
    } else {
        format!("{sign}{feet}' {}\"", inches_frac(inches))
    }
}

/// 5.375 -> "5 3/8"
fn inches_frac(x: f64) -> String {
    let eighths = (x.abs() * 8.0).round() as i64;
    let whole = eighths / 8;
    let mut num = eighths % 8;
    let mut den = 8;
    while num > 0 && num % 2 == 0 {
        num /= 2;
        den /= 2;
    }
    let sign = if x < 0.0 { "-" } else { "" };
    match (whole, num) {
        (w, 0) => format!("{sign}{w}"),
        (0, n) => format!("{sign}{n}/{den}"),
        (w, n) => format!("{sign}{w} {n}/{den}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats() {
        assert_eq!(format(3.5052, "ft"), "11' 6\"");
        assert_eq!(format(0.0254 * 5.375, "in"), "5 3/8\"");
        assert_eq!(format(2.5, "m"), "2.50 m");
        assert_eq!(format(0.3, "mm"), "300 mm");
        assert_eq!(format(0.0254 * 7.0, "ft"), "7\"");
    }
}
