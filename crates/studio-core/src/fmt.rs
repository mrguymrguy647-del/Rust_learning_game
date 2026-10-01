//! Small display helpers shared by the UI and tests.

/// `12345` → `$12,345`, `-5` → `-$5`.
pub fn money(amount: i64) -> String {
    let digits = amount.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3 + 2);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    if amount < 0 {
        format!("-${grouped}")
    } else {
        format!("${grouped}")
    }
}

/// `1_250_000` → `1.25M`, `9_500` → `9.5k`, `812` → `812`.
pub fn compact(n: i64) -> String {
    let abs = n.unsigned_abs() as f64;
    let sign = if n < 0 { "-" } else { "" };
    let (value, suffix) = if abs >= 1e9 {
        (abs / 1e9, "B")
    } else if abs >= 1e6 {
        (abs / 1e6, "M")
    } else if abs >= 1e4 {
        (abs / 1e3, "k")
    } else {
        return format!("{n}");
    };
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{sign}{text}{suffix}")
}

/// `0.734` → `73%`.
pub fn percent(fraction: f32) -> String {
    format!("{:.0}%", (fraction * 100.0).clamp(-999.0, 999.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_groups_thousands() {
        assert_eq!(money(0), "$0");
        assert_eq!(money(999), "$999");
        assert_eq!(money(1_000), "$1,000");
        assert_eq!(money(1_234_567), "$1,234,567");
        assert_eq!(money(-45_000), "-$45,000");
    }

    #[test]
    fn compact_numbers() {
        assert_eq!(compact(812), "812");
        assert_eq!(compact(9_999), "9999");
        assert_eq!(compact(12_500), "12.5k");
        assert_eq!(compact(1_250_000), "1.25M");
        assert_eq!(compact(-3_000_000), "-3M");
    }
}
