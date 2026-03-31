//! Centralized numeric conversion utilities.
//!
//! These functions isolate lossy or checked numeric casts to a single location,
//! satisfying clippy's strict `-D warnings` mode while keeping the call sites clean.

/// Safely convert an `f64` to `i64`, rounding toward zero.
///
/// Values outside `i64` range saturate.  The explicit range-check avoids
/// the `clippy::cast_possible_truncation` lint without `#[allow]`.
pub fn float_to_i64(val: f64) -> i64 {
    // -2^63 and 2^63-1 expressed as f64 literals to avoid i64→f64 casts.
    const MIN: f64 = -9_223_372_036_854_775_808.0;
    const MAX: f64 = 9_223_372_036_854_775_807.0;

    if val.is_nan() {
        return 0;
    }
    if val <= MIN {
        return i64::MIN;
    }
    if val >= MAX {
        return i64::MAX;
    }
    // SAFETY: `val` is finite and within (i64::MIN, i64::MAX).
    unsafe { val.to_int_unchecked::<i64>() }
}

/// Safely convert an f64 to u32, clamping to `[0, u32::MAX]`.
///
/// Uses `to_int_unchecked` after range-checking to avoid clippy casts.
#[inline]
pub fn f64_to_u32_saturating(value: f64) -> u32 {
    if value.is_nan() || value <= 0.0 {
        return 0;
    }
    let max = f64::from(u32::MAX);
    if value >= max {
        return u32::MAX;
    }
    // SAFETY: value is finite, non-negative, and strictly less than u32::MAX.
    unsafe { value.to_int_unchecked::<u32>() }
}

/// Convert a non-negative, clamped `f64` pixel value to `u32`.
/// The caller must guarantee `val >= 0.0`.
pub fn clamped_f64_to_u32(val: f64) -> u32 {
    debug_assert!(val >= 0.0);
    let clamped = val.floor().clamp(0.0, f64::from(u32::MAX));
    let int_val = float_to_i64(clamped);
    u32::try_from(int_val).expect("pixel coordinate fits u32")
}

/// Convert `f64` to `f32` for pixel coordinates.
///
/// Screen coordinates (0..~16384) are well within `f32` range.
/// Precision loss beyond ~7 significant digits is acceptable.
#[expect(
    clippy::cast_possible_truncation,
    reason = "intentional f64→f32 truncation; no TryFrom<f64> for f32 in std"
)]
pub const fn pixel_f64_to_f32(val: f64) -> f32 {
    val as f32
}

/// Convert an `i32` scroll delta to `isize` for viewport scrolling.
pub const fn scroll_delta_to_isize(val: i32) -> isize {
    val as isize
}

/// Convert a logical pixel value to physical (buffer) pixels at the given scale.
///
/// `scale_120` uses wp-fractional-scale-v1 convention: 120 = 1.0x, 240 = 2.0x, etc.
/// Rounds to nearest via `(logical * scale_120 + 60) / 120`.
pub fn phys_from_scale(logical: u32, scale_120: u32) -> u32 {
    (u64::from(logical) * u64::from(scale_120) + 60)
        .checked_div(120)
        .and_then(|v| u32::try_from(v).ok())
        .expect("physical pixel value fits u32")
}

/// Narrow a `u32` FFI constant to `i32`, panicking if it overflows.
pub fn u32_to_i32(val: u32) -> i32 {
    i32::try_from(val).expect("u32 value fits i32")
}

/// Saturate a `u64` into `u32` range.
pub fn u32_from_u64_saturating(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Convert a non-negative f32 (font metric) to u32.
/// Returns 0 for negative or NaN values.
///
/// Font metrics are small (typically < 200), well within the safe range.
pub fn f32_metric_to_u32(val: f32) -> u32 {
    let wide = f64::from(val);
    if wide <= 0.0 {
        return 0;
    }
    // 16_777_215 = 2^24 - 1, the largest integer exactly representable in f32.
    if wide >= 16_777_215.0 {
        return 16_777_215;
    }
    // wide is in (0.0, 16_777_215.0) -- safe to convert via i64.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "range-checked above: 0 < wide < 16_777_215"
    )]
    let int_val = wide.trunc() as i64;
    u32::try_from(int_val).unwrap_or(0)
}

/// Format a `usize` into a stack buffer, returning the resulting `&str`.
/// Avoids the heap allocation of `format!("{match_count}")`.
pub fn format_usize(mut n: usize, buf: &mut [u8; 20]) -> &str {
    if n == 0 {
        if let Some(slot) = buf.get_mut(19) {
            *slot = b'0';
        }
        return std::str::from_utf8(buf.get(19..).unwrap_or(&[])).expect("ASCII digit");
    }
    let mut pos = 20;
    while n > 0 {
        pos -= 1;
        if let Some(slot) = buf.get_mut(pos) {
            *slot = b'0' + u8::try_from(n % 10).expect("digit fits u8");
        }
        n /= 10;
    }
    std::str::from_utf8(buf.get(pos..).unwrap_or(&[])).expect("ASCII digits")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_float_to_i64_zero() {
        assert_eq!(float_to_i64(0.0), 0);
    }

    #[test]
    fn test_float_to_i64_positive() {
        assert_eq!(float_to_i64(42.0), 42);
        assert_eq!(float_to_i64(3.99), 3);
    }

    #[test]
    fn test_float_to_i64_negative() {
        assert_eq!(float_to_i64(-1.0), -1);
        assert_eq!(float_to_i64(-99.5), -99);
    }

    #[test]
    fn test_float_to_i64_nan() {
        assert_eq!(float_to_i64(f64::NAN), 0);
    }

    #[test]
    fn test_float_to_i64_infinity() {
        assert_eq!(float_to_i64(f64::INFINITY), i64::MAX);
        assert_eq!(float_to_i64(f64::NEG_INFINITY), i64::MIN);
    }

    #[test]
    fn test_float_to_i64_large_positive() {
        assert_eq!(float_to_i64(1e19), i64::MAX);
    }

    #[test]
    fn test_float_to_i64_large_negative() {
        assert_eq!(float_to_i64(-1e19), i64::MIN);
    }

    #[test]
    fn test_f64_to_u32_saturating_zero() {
        assert_eq!(f64_to_u32_saturating(0.0), 0);
    }

    #[test]
    fn test_f64_to_u32_saturating_negative() {
        assert_eq!(f64_to_u32_saturating(-100.0), 0);
    }

    #[test]
    fn test_f64_to_u32_saturating_nan() {
        assert_eq!(f64_to_u32_saturating(f64::NAN), 0);
    }

    #[test]
    fn test_f64_to_u32_saturating_overflow() {
        assert_eq!(f64_to_u32_saturating(5_000_000_000.0), u32::MAX);
    }

    #[test]
    fn test_f64_to_u32_saturating_normal() {
        assert_eq!(f64_to_u32_saturating(42.0), 42);
        assert_eq!(f64_to_u32_saturating(1000.0), 1000);
    }

    #[test]
    fn test_f64_to_u32_saturating_fractional() {
        assert_eq!(f64_to_u32_saturating(3.7), 3);
        assert_eq!(f64_to_u32_saturating(0.99), 0);
    }

    #[test]
    fn test_f64_to_u32_saturating_infinity() {
        assert_eq!(f64_to_u32_saturating(f64::INFINITY), u32::MAX);
    }

    #[test]
    fn test_f64_to_u32_saturating_neg_infinity() {
        assert_eq!(f64_to_u32_saturating(f64::NEG_INFINITY), 0);
    }

    #[test]
    fn test_clamped_f64_to_u32_zero() {
        assert_eq!(clamped_f64_to_u32(0.0), 0);
    }

    #[test]
    fn test_clamped_f64_to_u32_fractional() {
        assert_eq!(clamped_f64_to_u32(3.7), 3);
        assert_eq!(clamped_f64_to_u32(0.99), 0);
    }

    #[test]
    fn test_clamped_f64_to_u32_large() {
        assert_eq!(clamped_f64_to_u32(1920.0), 1920);
        assert_eq!(clamped_f64_to_u32(3840.0), 3840);
    }

    #[test]
    fn test_clamped_f64_to_u32_overflow() {
        assert_eq!(clamped_f64_to_u32(5_000_000_000.0), u32::MAX);
    }

    #[test]
    fn test_clamped_f64_to_u32_at_u32_max() {
        assert_eq!(clamped_f64_to_u32(f64::from(u32::MAX)), u32::MAX);
    }

    #[test]
    fn test_pixel_f64_to_f32_zero() {
        let result = pixel_f64_to_f32(0.0);
        assert!((result - 0.0_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn test_pixel_f64_to_f32_normal() {
        let result = pixel_f64_to_f32(1920.5);
        assert!((result - 1920.5_f32).abs() < 0.01);
    }

    #[test]
    fn test_pixel_f64_to_f32_precision() {
        let result = pixel_f64_to_f32(3840.0);
        assert!((result - 3840.0_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn test_scroll_delta_to_isize_zero() {
        assert_eq!(scroll_delta_to_isize(0), 0);
    }

    #[test]
    fn test_scroll_delta_to_isize_positive() {
        assert_eq!(scroll_delta_to_isize(5), 5);
    }

    #[test]
    fn test_scroll_delta_to_isize_negative() {
        assert_eq!(scroll_delta_to_isize(-3), -3);
    }

    #[test]
    fn test_scroll_delta_to_isize_extremes() {
        assert_eq!(
            scroll_delta_to_isize(i32::MAX),
            isize::try_from(i32::MAX).expect("fits")
        );
        assert_eq!(
            scroll_delta_to_isize(i32::MIN),
            isize::try_from(i32::MIN).expect("fits")
        );
    }

    #[test]
    fn test_phys_at_1x_scale() {
        assert_eq!(phys_from_scale(10, 120), 10);
    }

    #[test]
    fn test_phys_at_2x_scale() {
        assert_eq!(phys_from_scale(10, 240), 20);
    }

    #[test]
    fn test_phys_at_fractional_scale() {
        assert_eq!(phys_from_scale(100, 168), 140);
    }

    #[test]
    fn test_phys_zero() {
        assert_eq!(phys_from_scale(0, 120), 0);
        assert_eq!(phys_from_scale(0, 240), 0);
        assert_eq!(phys_from_scale(0, 168), 0);
        assert_eq!(phys_from_scale(0, 1), 0);
    }

    #[test]
    fn test_phys_from_scale_half() {
        assert_eq!(phys_from_scale(7, 180), 11);
    }

    #[test]
    fn test_phys_from_scale_rounding() {
        assert_eq!(phys_from_scale(3, 150), 4);
    }

    #[test]
    fn test_phys_from_scale_one_pixel() {
        assert_eq!(phys_from_scale(1, 120), 1);
        assert_eq!(phys_from_scale(1, 240), 2);
    }

    #[test]
    fn test_u32_to_i32_zero() {
        assert_eq!(u32_to_i32(0), 0);
    }

    #[test]
    fn test_u32_to_i32_max_valid() {
        let max = i32::MAX.cast_unsigned();
        assert_eq!(u32_to_i32(max), i32::MAX);
    }

    #[test]
    fn test_u32_to_i32_normal() {
        assert_eq!(u32_to_i32(1920), 1920);
        assert_eq!(u32_to_i32(100), 100);
    }

    #[test]
    fn test_u32_from_u64_saturating_normal() {
        assert_eq!(u32_from_u64_saturating(0), 0);
        assert_eq!(u32_from_u64_saturating(12345), 12345);
        assert_eq!(u32_from_u64_saturating(u64::from(u32::MAX)), u32::MAX);
    }

    #[test]
    fn test_u32_from_u64_saturating_overflow() {
        assert_eq!(u32_from_u64_saturating(u64::from(u32::MAX) + 1), u32::MAX);
        assert_eq!(u32_from_u64_saturating(u64::MAX), u32::MAX);
    }

    #[test]
    fn test_f32_metric_to_u32_negative() {
        assert_eq!(f32_metric_to_u32(-5.0), 0);
    }

    #[test]
    fn test_f32_metric_to_u32_zero() {
        assert_eq!(f32_metric_to_u32(0.0), 0);
    }

    #[test]
    fn test_f32_metric_to_u32_nan() {
        assert_eq!(f32_metric_to_u32(f32::NAN), 0);
    }

    #[test]
    fn test_f32_metric_to_u32_infinity() {
        assert_eq!(f32_metric_to_u32(f32::INFINITY), 16_777_215);
    }

    #[test]
    fn test_f32_metric_to_u32_normal() {
        assert_eq!(f32_metric_to_u32(10.5), 10);
    }

    #[test]
    fn test_format_usize_zero() {
        let mut buf = [0u8; 20];
        assert_eq!(format_usize(0, &mut buf), "0");
    }

    #[test]
    fn test_format_usize_large() {
        let mut buf = [0u8; 20];
        assert_eq!(format_usize(123_456, &mut buf), "123456");
    }

    #[test]
    fn test_format_usize_max() {
        let mut buf = [0u8; 20];
        let result = format_usize(usize::MAX, &mut buf);
        assert_eq!(result, usize::MAX.to_string());
    }

    #[test]
    fn test_format_usize_single_digit() {
        let mut buf = [0u8; 20];
        assert_eq!(format_usize(7, &mut buf), "7");
    }

    #[test]
    fn test_format_usize_powers_of_ten() {
        let mut buf = [0u8; 20];
        assert_eq!(format_usize(10, &mut buf), "10");
        assert_eq!(format_usize(100, &mut buf), "100");
        assert_eq!(format_usize(1000, &mut buf), "1000");
    }
}
