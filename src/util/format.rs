/// Formats a floating-point number as a human-readable string with fractions
/// Based on the approach from cooklang-rs/bindings/src/lib.rs
pub fn format_number(value: f64) -> String {
    // Round to reasonable precision to handle floating point errors
    // This handles cases like 0.89999999999 -> 0.9
    let rounded = (value * 1000000.0).round() / 1000000.0;

    // Check if it's effectively a whole number
    if (rounded.fract()).abs() < 0.0000001 {
        return format!("{rounded:.0}");
    }

    // Try to convert to a common fraction
    if let Some(fraction) = decimal_to_fraction(rounded) {
        return fraction;
    }

    // For decimals, determine appropriate precision
    // Round to at most 3 decimal places, but remove trailing zeros
    let rounded_to_3 = (rounded * 1000.0).round() / 1000.0;

    // Format with appropriate precision
    let mut result = if (rounded_to_3 * 100.0).fract().abs() < 0.001 {
        // Has at most 2 decimal places
        format!("{rounded_to_3:.2}")
    } else {
        // Needs 3 decimal places
        format!("{rounded_to_3:.3}")
    };

    // Remove trailing zeros and decimal point if not needed
    if result.contains('.') {
        result = result
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
    }

    result
}

/// Converts common decimal values to fraction strings
fn decimal_to_fraction(value: f64) -> Option<String> {
    const EPSILON: f64 = 0.0001;

    // Split into whole and fractional parts
    let whole = value.floor();
    let fract = value - whole;

    // Common fractions and their decimal equivalents
    let common_fractions = [
        (0.125, "1/8"),
        (0.25, "1/4"),
        (0.333333, "1/3"),
        (0.375, "3/8"),
        (0.5, "1/2"),
        (0.625, "5/8"),
        (0.666667, "2/3"),
        (0.75, "3/4"),
        (0.875, "7/8"),
    ];

    // Check if the fractional part matches any common fraction
    for &(decimal, fraction_str) in &common_fractions {
        if (fract - decimal).abs() < EPSILON {
            if whole > 0.0 {
                // For values > 1, return decimal format instead of mixed fraction
                return None;
            } else {
                return Some(fraction_str.to_string());
            }
        }
    }

    None
}

/// Parses a servings value and scales the leading number.
/// The leading number is used for scaling; anything else is ignored for scaling but shown as units.
/// Examples:
///   "4 people" scaled by 2.0 -> "8 people"
///   "4" scaled by 2.0 -> "8"
///   "4.5 servings" scaled by 0.5 -> "2.25 servings"
pub fn scale_servings(raw: &str, scale: f64) -> String {
    let trimmed = raw.trim();

    // Find the end of the leading number (digits and decimal point)
    let num_end = trimmed
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(trimmed.len());

    let (num_str, rest) = trimmed.split_at(num_end);

    if let Ok(n) = num_str.parse::<f64>() {
        let scaled = n * scale;
        let formatted = format_number(scaled);
        let rest_trimmed = rest.trim();
        if rest_trimmed.is_empty() {
            formatted
        } else {
            format!("{formatted} {rest_trimmed}")
        }
    } else {
        // No leading number found; return as-is
        raw.to_string()
    }
}

/// Formats a quantity value for display
pub fn format_quantity(value: &cooklang::Value) -> Option<String> {
    match value {
        cooklang::Value::Number(n) => Some(format_number(n.value())),
        cooklang::Value::Range { start, end } => Some(format!(
            "{} - {}",
            format_number(start.value()),
            format_number(end.value())
        )),
        cooklang::Value::Text(s) => Some(s.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_fractions() {
        // Fractions less than 1 should be displayed as fractions
        assert_eq!(format_number(0.5), "1/2");
        assert_eq!(format_number(0.25), "1/4");
        assert_eq!(format_number(0.75), "3/4");
        assert_eq!(format_number(0.333333), "1/3");
        assert_eq!(format_number(0.666667), "2/3");

        // Values greater than 1 should be displayed as decimals
        assert_eq!(format_number(1.5), "1.5");
        assert_eq!(format_number(2.25), "2.25");
        assert_eq!(format_number(1.75), "1.75");
        assert_eq!(format_number(2.333333), "2.333");
    }

    #[test]
    fn test_format_whole_numbers() {
        assert_eq!(format_number(2.0), "2");
        assert_eq!(format_number(1.9999999999), "2");
    }

    #[test]
    fn test_format_decimals() {
        assert_eq!(format_number(1.23), "1.23");
        assert_eq!(format_number(0.899), "0.899");
        assert_eq!(format_number(0.89999999999), "0.9");
        assert_eq!(format_number(0.30000000001), "0.3");
    }
}
