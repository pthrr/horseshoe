use crate::num::f32_metric_to_u32;
use fontdue::{Font, FontSettings};
use std::collections::HashMap;

static FONT_REGULAR: &[u8] = include_bytes!("../data/fonts/JetBrainsMono-Regular.ttf");
static FONT_BOLD: &[u8] = include_bytes!("../data/fonts/JetBrainsMono-Bold.ttf");
static FONT_ITALIC: &[u8] = include_bytes!("../data/fonts/JetBrainsMono-Italic.ttf");
static FONT_BOLD_ITALIC: &[u8] = include_bytes!("../data/fonts/JetBrainsMono-BoldItalic.ttf");

static FONT_EMOJI: &[u8] = include_bytes!("../data/fonts/NotoEmoji-Regular.ttf");

/// Cached glyph image with alpha coverage data.
#[derive(Clone)]
pub struct GlyphImage {
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
    /// Alpha values, row-major, one byte per pixel.
    pub alpha: Vec<u8>,
}

/// Style variant index: 0=regular, 1=bold, 2=italic, 3=bold+italic.
#[inline]
fn style_index(bold: bool, italic: bool) -> usize {
    usize::from(bold) | (usize::from(italic) << 1)
}

/// Load a `fontdue::Font` from raw bytes, returning `None` on failure.
fn load_font(data: &[u8], scale: f32) -> Option<Font> {
    let settings = FontSettings {
        scale,
        ..FontSettings::default()
    };
    Font::from_bytes(data, settings).ok()
}

/// Normalize a font name for matching: lowercase, strip spaces.
fn normalize_font_name(name: &str) -> String {
    name.to_lowercase().replace(' ', "")
}

/// Well-known system fonts that cover Unicode symbol blocks (Misc Symbols,
/// Geometric Shapes, Dingbats, etc.) not included in `NotoEmoji`.
const FALLBACK_FAMILIES: &[&str] = &[
    "NotoSans",
    "NotoSansSymbols",
    "NotoSansSymbols2",
    "DejaVuSans",
    "Symbola",
    "FreeSans",
];

/// Scan system font directories for well-known symbol/fallback fonts.
/// Returns only Regular-style fonts suitable for appending to every chain.
fn scan_fallback_fonts(scale: f32) -> Vec<Font> {
    let families_norm: Vec<String> = FALLBACK_FAMILIES
        .iter()
        .map(|f| normalize_font_name(f))
        .collect();

    let dirs = ["/usr/share/fonts", "/usr/local/share/fonts"];
    let home_dir = std::env::var_os("HOME").map(|h| {
        let mut p = std::path::PathBuf::from(h);
        p.push(".local/share/fonts");
        p
    });

    let mut fonts = Vec::new();
    for dir_path in dirs.iter().map(std::path::PathBuf::from).chain(home_dir) {
        scan_fallback_dir(&dir_path, &families_norm, scale, &mut fonts);
    }
    fonts
}

/// Recursively scan a directory for fonts matching any of the fallback families.
/// Only Regular style is collected.
fn scan_fallback_dir(
    dir: &std::path::Path,
    families_norm: &[String],
    scale: f32,
    result: &mut Vec<Font>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_fallback_dir(&path, families_norm, scale, result);
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !matches!(ext, "ttf" | "otf" | "ttc") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let name_norm = normalize_font_name(name);

        // Check if this file matches any fallback family (Regular style only)
        let matched = families_norm
            .iter()
            .any(|fam| matches_font_family(&name_norm, fam));
        if !matched {
            continue;
        }
        // Only accept Regular style
        let family_norm = families_norm
            .iter()
            .find(|fam| matches_font_family(&name_norm, fam));
        if let Some(fam) = family_norm
            && classify_font_style(&name_norm, fam) != Some(style_index(false, false))
        {
            continue;
        }
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        let Some(font) = load_font(&data, scale) else {
            continue;
        };
        result.push(font);
    }
}

/// Check if a normalized filename matches a font family.
/// The filename (without extension) must start with the family name,
/// followed by `-` (style suffix) or end of string.
/// This prevents "jetbrainsmono" from matching "jetbrainsmononerdfont".
fn matches_font_family(filename_norm: &str, family_norm: &str) -> bool {
    if let Some(rest) = filename_norm.strip_prefix(family_norm) {
        rest.is_empty() || rest.starts_with('-')
    } else {
        false
    }
}

/// Scan common system font directories for files whose name contains `family`.
/// Returns loaded fonts grouped as (regular, bold, italic, bold-italic) candidates.
fn scan_system_fonts(family: &str, scale: f32) -> [Vec<Font>; 4] {
    let mut result: [Vec<Font>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    let family_lower = normalize_font_name(family);

    let dirs = ["/usr/share/fonts", "/usr/local/share/fonts"];

    let home_dir = std::env::var_os("HOME").map(|h| {
        let mut p = std::path::PathBuf::from(h);
        p.push(".local/share/fonts");
        p
    });

    for dir_path in dirs.iter().map(std::path::PathBuf::from).chain(home_dir) {
        scan_font_dir(&dir_path, &family_lower, scale, &mut result);
    }

    result
}

fn scan_font_dir(
    dir: &std::path::Path,
    family_lower: &str,
    scale: f32,
    result: &mut [Vec<Font>; 4],
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_font_dir(&path, family_lower, scale, result);
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !matches!(ext, "ttf" | "otf" | "ttc") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let name_norm = normalize_font_name(name);
        if !matches_font_family(&name_norm, family_lower) {
            continue;
        }
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        let Some(font) = load_font(&data, scale) else {
            continue;
        };
        let Some(idx) = classify_font_style(&name_norm, family_lower) else {
            continue;
        };
        if let Some(chain) = result.get_mut(idx) {
            chain.push(font);
        }
    }
}

/// Classify a font filename into a style index based on the suffix after the
/// family name.  Only the four standard weights are accepted: Regular, Bold,
/// Italic, and `BoldItalic` (or Oblique variants).  Other weights (Thin, Light,
/// Medium, `SemiBold`, `ExtraBold`, etc.) are skipped to avoid polluting the
/// font chains with unexpected weights.
fn classify_font_style(name_norm: &str, family_norm: &str) -> Option<usize> {
    let after_family = name_norm.strip_prefix(family_norm).unwrap_or(name_norm);
    let style = after_family.strip_prefix('-').unwrap_or(after_family);
    match style {
        "" | "regular" => Some(style_index(false, false)),
        "bold" => Some(style_index(true, false)),
        "italic" | "oblique" => Some(style_index(false, true)),
        "bolditalic" | "boldoblique" | "italicbold" | "obliquebold" => {
            Some(style_index(true, true))
        }
        _ => None,
    }
}

pub struct FontManager {
    /// Font chains per style: regular, bold, italic, bold-italic.
    /// Each chain is tried in order; first font with `has_glyph` wins.
    fonts: [Vec<Font>; 4],
    /// Glyph cache keyed by `(codepoint, style_bits)` — no String allocation.
    char_cache: HashMap<(u32, u8), Option<GlyphImage>>,
    /// Cell width in pixels.
    pub cell_width: u32,
    /// Cell height in pixels.
    pub cell_height: u32,
    font_size: f32,
}

impl FontManager {
    /// Create a new font manager.
    ///
    /// If `family` is provided, system fonts are searched for that family
    /// first; the embedded monospace font is always available as
    /// a fallback.  `font_size` is the desired font size in pixels.
    pub fn new_with_family(font_size: f32, family: Option<&str>) -> Self {
        let settings = FontSettings {
            scale: font_size,
            ..FontSettings::default()
        };

        let emb_regular = Font::from_bytes(FONT_REGULAR, settings).expect("embedded regular font");
        let emb_bold = Font::from_bytes(FONT_BOLD, settings).expect("embedded bold font");
        let emb_italic = Font::from_bytes(FONT_ITALIC, settings).expect("embedded italic font");
        let emb_bold_italic =
            Font::from_bytes(FONT_BOLD_ITALIC, settings).expect("embedded bold italic font");
        let emb_emoji = Font::from_bytes(FONT_EMOJI, settings).expect("embedded emoji font");

        // Build fallback chains: [system fonts..., embedded, emoji, ...symbol fallbacks]
        let mut sys = if let Some(fam) = family {
            scan_system_fonts(fam, font_size)
        } else {
            [Vec::new(), Vec::new(), Vec::new(), Vec::new()]
        };

        let fallback_fonts = scan_fallback_fonts(font_size);

        let mut fonts: [Vec<Font>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        let embedded = [emb_regular, emb_bold, emb_italic, emb_bold_italic];
        for ((chain, sys_chain), emb) in fonts.iter_mut().zip(sys.iter_mut()).zip(embedded) {
            chain.append(sys_chain);
            chain.push(emb);
            chain.push(emb_emoji.clone());
            chain.extend(fallback_fonts.iter().cloned());
        }

        // 'M' width defines the cell grid — monospace fonts have uniform advance.
        let m_metrics = fonts[0]
            .first()
            .expect("at least embedded font")
            .metrics('M', font_size);
        let cell_width = f32_metric_to_u32(m_metrics.advance_width);

        let line_metrics = fonts[0]
            .first()
            .expect("at least embedded font")
            .horizontal_line_metrics(font_size);
        let cell_height = if let Some(lm) = line_metrics {
            f32_metric_to_u32(lm.new_line_size)
        } else {
            f32_metric_to_u32(font_size * 1.2)
        };

        FontManager {
            fonts,
            char_cache: HashMap::new(),
            cell_width: cell_width.max(1),
            cell_height: cell_height.max(1),
            font_size,
        }
    }

    /// Rebuild at a new size without re-scanning the filesystem.
    ///
    /// Clears the glyph cache and recalculates cell metrics using the
    /// already-loaded font chains.  Much faster than `new_with_family()`
    /// because it skips the recursive directory scan.
    pub fn rebuild_at_size(&mut self, new_size: f32) {
        self.font_size = new_size;
        self.char_cache.clear();

        let m_metrics = self.fonts[0]
            .first()
            .expect("at least embedded font")
            .metrics('M', new_size);
        self.cell_width = f32_metric_to_u32(m_metrics.advance_width).max(1);

        let line_metrics = self.fonts[0]
            .first()
            .expect("at least embedded font")
            .horizontal_line_metrics(new_size);
        self.cell_height = if let Some(lm) = line_metrics {
            f32_metric_to_u32(lm.new_line_size).max(1)
        } else {
            f32_metric_to_u32(new_size * 1.2).max(1)
        };
    }

    /// Create a new font manager with the default embedded font family.
    #[cfg(test)]
    pub fn new(font_size: f32) -> Self {
        Self::new_with_family(font_size, None)
    }

    /// Rasterize a grapheme cluster with the given style.
    /// Returns a reference to the cached `GlyphImage`, or None if rasterization failed.
    ///
    /// All lookups go through the `(codepoint, style)` cache — no String allocation.
    /// Multi-codepoint clusters rasterize the first codepoint only (fontdue does not
    /// support OpenType ligatures or ZWJ sequences).
    pub fn rasterize(&mut self, text: &str, bold: bool, italic: bool) -> Option<&GlyphImage> {
        let first_ch = text.chars().next()?;
        self.rasterize_char(first_ch as u32, bold, italic)
    }

    /// Fast-path rasterization for single codepoints (avoids String allocation).
    fn rasterize_char(&mut self, cp: u32, bold: bool, italic: bool) -> Option<&GlyphImage> {
        let style_bits = u8::from(bold) | (u8::from(italic) << 1);
        let key = (cp, style_bits);

        // Extract chain slice and font_size before entry() to satisfy borrow checker.
        let chain = self
            .fonts
            .get(style_index(bold, italic))
            .map_or(&[][..], Vec::as_slice);
        let font_size = self.font_size;

        self.char_cache
            .entry(key)
            .or_insert_with(|| {
                char::from_u32(cp).and_then(|ch| rasterize_glyph(chain, ch, font_size))
            })
            .as_ref()
    }

    /// Clear the glyph cache.
    pub fn clear_cache(&mut self) {
        self.char_cache.clear();
    }
}

/// Rasterize a character using the first font in the chain that has the glyph.
fn rasterize_glyph(chain: &[Font], ch: char, font_size: f32) -> Option<GlyphImage> {
    // Find the first font in the chain that has this glyph
    let font = chain
        .iter()
        .find(|f| f.has_glyph(ch))
        .or_else(|| chain.first())?;

    let (metrics, alpha) = font.rasterize(ch, font_size);

    if metrics.width == 0 || metrics.height == 0 {
        return None;
    }

    let width = u32::try_from(metrics.width).expect("glyph width fits u32");
    let height = u32::try_from(metrics.height).expect("glyph height fits u32");
    let height_i32 = i32::try_from(metrics.height).expect("glyph height fits i32");

    Some(GlyphImage {
        width,
        height,
        left: metrics.xmin,
        top: height_i32 + metrics.ymin,
        alpha,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_dimensions_reasonable() {
        let fm = FontManager::new(16.0);
        // Cell width should be roughly 60% of font size for monospace
        assert!(
            fm.cell_width >= 5 && fm.cell_width <= 20,
            "cell_width {} out of expected range",
            fm.cell_width
        );
        // Cell height should be roughly 120% of font size
        assert!(
            fm.cell_height >= 10 && fm.cell_height <= 30,
            "cell_height {} out of expected range",
            fm.cell_height
        );
    }

    #[test]
    fn test_rasterize_letter() {
        let mut fm = FontManager::new(16.0);
        let img = fm.rasterize("A", false, false);
        assert!(img.is_some());
        let glyph = img.expect("rasterize should return a glyph for 'A'");
        assert!(glyph.width > 0);
        assert!(glyph.height > 0);
        assert!(!glyph.alpha.is_empty());
    }

    #[test]
    fn test_rasterize_style_variants() {
        let mut fm = FontManager::new(16.0);
        assert!(fm.rasterize("B", true, false).is_some(), "bold");
        assert!(fm.rasterize("I", false, true).is_some(), "italic");
        assert!(fm.rasterize("X", true, true).is_some(), "bold italic");
    }

    #[test]
    fn test_glyph_cache_returns_consistent_results() {
        let mut fm = FontManager::new(16.0);
        // First call populates the cache
        let img1 = fm.rasterize("X", false, false).map(|g| (g.width, g.height));
        // Second call should hit the cache
        let img2 = fm.rasterize("X", false, false).map(|g| (g.width, g.height));
        assert_eq!(img1, img2);
    }

    #[test]
    fn test_rasterize_different_styles_cached_separately() {
        let mut fm = FontManager::new(16.0);
        let _ = fm.rasterize("A", false, false);
        let _ = fm.rasterize("A", true, false);
        let _ = fm.rasterize("A", false, true);
        // All three should be in the char_cache now (single codepoint fast path)
        assert!(fm.char_cache.len() >= 3);
    }

    #[test]
    fn test_clear_cache() {
        let mut fm = FontManager::new(16.0);
        let _ = fm.rasterize("Z", false, false);
        assert!(!fm.char_cache.is_empty());
        fm.clear_cache();
        assert!(fm.char_cache.is_empty());
    }

    #[test]
    fn test_rasterize_space() {
        let mut fm = FontManager::new(16.0);
        // Space typically has no visible glyph data
        let img = fm.rasterize(" ", false, false);
        // It's okay for space to return None (no visible pixels)
        // Just ensure it doesn't panic
        let _ = img;
    }

    #[test]
    fn test_new_with_family_none_uses_embedded() {
        let fm = FontManager::new_with_family(16.0, None);
        assert!(fm.cell_width > 0);
        assert!(fm.cell_height > 0);
    }

    #[test]
    fn test_rasterize_nonexistent_glyph() {
        let mut fm = FontManager::new(16.0);
        // Private Use Area codepoint — unlikely to have a glyph
        let result = fm.rasterize("\u{FFFF}", false, false);
        // It's acceptable to return None for unrenderable codepoints
        let _ = result;
    }

    #[test]
    fn test_cell_dimensions_scale_with_size() {
        let fm12 = FontManager::new(12.0);
        let fm24 = FontManager::new(24.0);
        assert!(
            fm24.cell_width > fm12.cell_width,
            "24px font should have wider cells than 12px"
        );
        assert!(
            fm24.cell_height > fm12.cell_height,
            "24px font should have taller cells than 12px"
        );
    }

    #[test]
    fn test_rasterize_control_char_returns_none() {
        let mut fm = FontManager::new(16.0);
        // Null byte should return None or produce no visible glyph
        let img = fm.rasterize("\x00", false, false);
        assert!(
            img.is_none(),
            "null byte should not produce a visible glyph"
        );
    }

    #[test]
    fn test_new_with_nonexistent_family_fallback() {
        let fm = FontManager::new_with_family(16.0, Some("ThisFontDoesNotExist99"));
        assert!(
            fm.cell_width > 0,
            "cell_width should be > 0 even with nonexistent family"
        );
        assert!(
            fm.cell_height > 0,
            "cell_height should be > 0 even with nonexistent family"
        );
    }

    #[test]
    fn test_rasterized_glyph_alpha_matches_dimensions() {
        let mut fm = FontManager::new(16.0);
        let img = fm.rasterize("X", false, false);
        let glyph = img.expect("rasterize should return a glyph for X");
        assert_eq!(
            glyph.alpha.len(),
            (glyph.width * glyph.height) as usize,
            "alpha buffer length should equal width * height"
        );
    }

    #[test]
    fn test_rebuild_at_size_updates_metrics() {
        let mut fm = FontManager::new(12.0);
        let w12 = fm.cell_width;
        let h12 = fm.cell_height;

        fm.rebuild_at_size(24.0);
        assert!(
            fm.cell_width > w12,
            "24px cell_width ({}) should be larger than 12px ({})",
            fm.cell_width,
            w12
        );
        assert!(
            fm.cell_height > h12,
            "24px cell_height ({}) should be larger than 12px ({})",
            fm.cell_height,
            h12
        );
    }

    #[test]
    fn test_rebuild_at_size_clears_cache() {
        let mut fm = FontManager::new(16.0);
        let _ = fm.rasterize("A", false, false);
        assert!(!fm.char_cache.is_empty(), "cache should have entries");

        fm.rebuild_at_size(20.0);
        assert!(
            fm.char_cache.is_empty(),
            "cache should be cleared after rebuild"
        );

        // Rasterization should still work after rebuild
        let img = fm.rasterize("A", false, false);
        assert!(img.is_some(), "rasterize should work after rebuild");
    }

    #[test]
    fn test_multi_codepoint_no_panic() {
        let mut fm = FontManager::new(16.0);
        // Multi-codepoint strings (emoji ZWJ sequences, combining chars)
        // should not panic — they take the slow path and rasterize first cp.
        let _ = fm.rasterize("e\u{0301}", false, false); // e + combining acute
        let _ = fm.rasterize("\u{1F468}\u{200D}\u{1F469}", false, false); // family emoji
    }

    #[test]
    fn test_rasterize_all_printable_ascii() {
        let mut fm = FontManager::new(16.0);
        for cp in 0x20u32..=0x7E {
            let ch = char::from_u32(cp).expect("valid ASCII");
            let text = ch.to_string();
            // Should not panic for any printable ASCII character
            let _ = fm.rasterize(&text, false, false);
        }
    }

    #[test]
    fn test_rasterize_box_drawing_char() {
        let mut fm = FontManager::new(16.0);
        // Box drawing chars may not have font glyphs (they're drawn procedurally)
        // but rasterize should not panic
        let _ = fm.rasterize("─", false, false); // U+2500
        let _ = fm.rasterize("│", false, false); // U+2502
        let _ = fm.rasterize("█", false, false); // U+2588
    }

    #[test]
    fn test_glyph_dimensions_bounded() {
        let mut fm = FontManager::new(16.0);
        let max_w = fm.cell_width * 4;
        let max_h = fm.cell_height * 4;
        for &ch in &['A', 'M', 'W', 'g', 'y', '|'] {
            let text = ch.to_string();
            // Copy dimensions before dropping the borrow
            let dims = fm
                .rasterize(&text, false, false)
                .map(|g| (g.width, g.height));
            if let Some((w, h)) = dims {
                assert!(
                    w <= max_w,
                    "glyph '{ch}' width {w} exceeds 4x cell_width {max_w}"
                );
                assert!(
                    h <= max_h,
                    "glyph '{ch}' height {h} exceeds 4x cell_height {max_h}"
                );
            }
        }
    }

    #[test]
    fn test_style_index_all_four() {
        assert_eq!(style_index(false, false), 0);
        assert_eq!(style_index(true, false), 1);
        assert_eq!(style_index(false, true), 2);
        assert_eq!(style_index(true, true), 3);
    }

    #[test]
    fn test_rasterize_cache_idempotent() {
        let mut fm = FontManager::new(16.0);
        let dims1 = fm
            .rasterize("K", false, false)
            .map(|g| (g.width, g.height, g.alpha.len()));
        let dims2 = fm
            .rasterize("K", false, false)
            .map(|g| (g.width, g.height, g.alpha.len()));
        assert_eq!(dims1, dims2, "two calls should return identical data");
    }

    #[test]
    fn test_rebuild_invalidates_cache() {
        let mut fm = FontManager::new(16.0);
        let _ = fm.rasterize("R", false, false);
        assert!(!fm.char_cache.is_empty());
        fm.rebuild_at_size(20.0);
        assert!(
            fm.char_cache.is_empty(),
            "cache should be empty after rebuild"
        );
        // New rasterization should still work
        let glyph = fm.rasterize("R", false, false);
        assert!(glyph.is_some());
    }

    #[test]
    fn test_rasterize_empty_string() {
        let mut fm = FontManager::new(16.0);
        let result = fm.rasterize("", false, false);
        assert!(result.is_none(), "empty string should return None");
    }

    #[test]
    fn test_rasterize_multi_codepoint_returns_first_only() {
        let mut fm = FontManager::new(16.0);
        let dims_ab = fm
            .rasterize("AB", false, false)
            .map(|g| (g.width, g.height));
        let dims_a = fm.rasterize("A", false, false).map(|g| (g.width, g.height));
        assert_eq!(
            dims_ab, dims_a,
            "multi-codepoint 'AB' should produce same glyph as 'A' (first codepoint only)"
        );
    }

    #[test]
    fn test_glyph_image_dimensions_match_alpha() {
        let mut fm = FontManager::new(16.0);
        let glyph = fm
            .rasterize("A", false, false)
            .expect("rasterize should return a glyph for 'A'");
        assert_eq!(
            glyph.alpha.len(),
            (glyph.width * glyph.height) as usize,
            "alpha buffer length must equal width * height"
        );
    }

    #[test]
    fn test_rasterize_combining_sequences() {
        let mut fm = FontManager::new(16.0);
        // e + combining grave + combining acute — should not panic
        let result = fm.rasterize("e\u{0300}\u{0301}", false, false);
        assert!(
            result.is_some(),
            "combining sequence should rasterize the base character"
        );
    }

    #[test]
    fn test_rasterize_emoji_fallback() {
        let mut fm = FontManager::new(16.0);
        // U+1F600 GRINNING FACE — should fall back to Noto Emoji
        let result = fm.rasterize("\u{1F600}", false, false);
        assert!(
            result.is_some(),
            "emoji should be rasterized via Noto Emoji fallback"
        );
    }

    #[test]
    fn test_rebuild_at_size_very_small() {
        let mut fm = FontManager::new(16.0);
        fm.rebuild_at_size(0.5);
        assert!(
            fm.cell_width >= 1,
            "cell_width must be >= 1 even at tiny size, got {}",
            fm.cell_width
        );
        assert!(
            fm.cell_height >= 1,
            "cell_height must be >= 1 even at tiny size, got {}",
            fm.cell_height
        );
    }

    #[test]
    fn test_rebuild_at_size_very_large() {
        let mut fm = FontManager::new(16.0);
        fm.rebuild_at_size(200.0);
        assert!(
            fm.cell_width > 0,
            "cell_width must be > 0 at large size, got {}",
            fm.cell_width
        );
        assert!(
            fm.cell_height > 0,
            "cell_height must be > 0 at large size, got {}",
            fm.cell_height
        );
    }

    #[test]
    fn test_rebuild_at_size_same_size_clears_cache() {
        let mut fm = FontManager::new(16.0);
        // Populate cache
        let _ = fm.rasterize("A", false, false);
        assert!(
            !fm.char_cache.is_empty(),
            "cache should have entries after rasterize"
        );

        // Rebuild at the same size — cache must still be cleared
        fm.rebuild_at_size(16.0);
        assert!(
            fm.char_cache.is_empty(),
            "cache should be cleared even when rebuilding at the same size"
        );

        // Re-rasterize should produce fresh data
        let glyph = fm
            .rasterize("A", false, false)
            .expect("rasterize should work after same-size rebuild");
        assert!(glyph.width > 0);
        assert!(glyph.height > 0);
    }

    #[test]
    fn test_classify_font_style_variants() {
        let fam = "myfont";
        // Standard four weights are accepted
        assert_eq!(classify_font_style("myfont-bold", fam), Some(1));
        assert_eq!(classify_font_style("myfont-italic", fam), Some(2));
        assert_eq!(classify_font_style("myfont-bolditalic", fam), Some(3));
        assert_eq!(classify_font_style("myfont-regular", fam), Some(0));
        assert_eq!(classify_font_style("myfont-oblique", fam), Some(2));
        assert_eq!(classify_font_style("myfont-boldoblique", fam), Some(3));
        // Family name alone (no suffix) → regular
        assert_eq!(classify_font_style("myfont", fam), Some(0));
        // Non-standard weights are skipped
        assert_eq!(classify_font_style("myfont-thin", fam), None);
        assert_eq!(classify_font_style("myfont-light", fam), None);
        assert_eq!(classify_font_style("myfont-medium", fam), None);
        assert_eq!(classify_font_style("myfont-semibold", fam), None);
        assert_eq!(classify_font_style("myfont-extrabold", fam), None);
        assert_eq!(classify_font_style("myfont-extralight", fam), None);
        assert_eq!(classify_font_style("myfont-thinitalic", fam), None);
    }

    #[test]
    fn test_cell_dimensions_proportional() {
        let fm16 = FontManager::new(16.0);
        let fm32 = FontManager::new(32.0);

        // At double size, cell dimensions should be roughly 2x (within 20%)
        let width_ratio = f64::from(fm32.cell_width) / f64::from(fm16.cell_width);
        let height_ratio = f64::from(fm32.cell_height) / f64::from(fm16.cell_height);

        assert!(
            (1.6..=2.4).contains(&width_ratio),
            "width ratio at 32/16 should be ~2.0, got {width_ratio:.2}"
        );
        assert!(
            (1.6..=2.4).contains(&height_ratio),
            "height ratio at 32/16 should be ~2.0, got {height_ratio:.2}"
        );
    }

    #[test]
    fn test_load_font_with_valid_data() {
        // Exercise load_font directly with embedded regular font data
        let font = load_font(FONT_REGULAR, 16.0);
        assert!(font.is_some(), "embedded regular font data should load");
    }

    #[test]
    fn test_load_font_with_garbage_data() {
        let garbage = b"this is definitely not a font file";
        let font = load_font(garbage, 16.0);
        assert!(font.is_none(), "garbage data should fail to load");
    }

    #[test]
    fn test_load_font_with_empty_data() {
        let font = load_font(&[], 16.0);
        assert!(font.is_none(), "empty data should fail to load");
    }

    #[test]
    fn test_scan_font_dir_nonexistent_dir() {
        let mut result: [Vec<Font>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        let nonexistent = std::path::PathBuf::from("/tmp/hand_test_nonexistent_font_dir_99999");
        scan_font_dir(&nonexistent, "anything", 16.0, &mut result);
        // Should not panic, and result should remain empty
        for chain in &result {
            assert!(
                chain.is_empty(),
                "no fonts should be found in nonexistent dir"
            );
        }
    }

    #[test]
    fn test_scan_font_dir_with_non_font_file() {
        // Create a temp dir with a .ttf file that contains garbage data
        let dir = std::env::temp_dir().join("hand_test_scan_font_dir");
        let _ = std::fs::create_dir_all(&dir);
        let fake_ttf = dir.join("FakeFont-Regular.ttf");
        std::fs::write(&fake_ttf, b"not a real font").expect("write fake ttf");

        let mut result: [Vec<Font>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        scan_font_dir(&dir, "fakefont", 16.0, &mut result);

        // The file matches the family name but load_font fails, so no fonts added
        for chain in &result {
            assert!(
                chain.is_empty(),
                "invalid font data should not produce loaded fonts"
            );
        }

        // Cleanup
        let _ = std::fs::remove_file(&fake_ttf);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_scan_font_dir_skips_non_matching_family() {
        // Create a temp dir with a .ttf that does NOT match the requested family
        let dir = std::env::temp_dir().join("hand_test_scan_font_dir_nomatch");
        let _ = std::fs::create_dir_all(&dir);
        let ttf = dir.join("UnrelatedFont-Regular.ttf");
        std::fs::write(&ttf, b"not a font").expect("write ttf");

        let mut result: [Vec<Font>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        scan_font_dir(&dir, "jetbrains", 16.0, &mut result);

        for chain in &result {
            assert!(chain.is_empty(), "non-matching family should be skipped");
        }

        let _ = std::fs::remove_file(&ttf);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_scan_font_dir_skips_non_font_extensions() {
        let dir = std::env::temp_dir().join("hand_test_scan_font_dir_ext");
        let _ = std::fs::create_dir_all(&dir);
        let txt = dir.join("MyFont-Regular.txt");
        std::fs::write(&txt, b"not a font").expect("write txt");

        let mut result: [Vec<Font>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        scan_font_dir(&dir, "myfont", 16.0, &mut result);

        for chain in &result {
            assert!(chain.is_empty(), ".txt extension should be skipped");
        }

        let _ = std::fs::remove_file(&txt);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_scan_font_dir_recurses_subdirs() {
        let dir = std::env::temp_dir().join("hand_test_scan_font_dir_recurse");
        let subdir = dir.join("subdir");
        let _ = std::fs::create_dir_all(&subdir);
        let ttf = subdir.join("TestFont-Bold.ttf");
        std::fs::write(&ttf, b"fake font data").expect("write ttf");

        let mut result: [Vec<Font>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        scan_font_dir(&dir, "testfont", 16.0, &mut result);

        // load_font will fail on fake data, but the code path through
        // subdirectory recursion and name matching is exercised.
        // No fonts actually loaded since data is invalid.
        let _ = std::fs::remove_file(&ttf);
        let _ = std::fs::remove_dir(&subdir);
        let _ = std::fs::remove_dir(&dir);
    }

    // ---- Fallback font tests (Bug 1: missing Unicode glyphs) ----

    #[test]
    fn test_fallback_families_not_empty() {
        assert!(
            !FALLBACK_FAMILIES.is_empty(),
            "FALLBACK_FAMILIES should contain at least one family"
        );
    }

    #[test]
    fn test_fallback_families_contains_expected() {
        // Verify the known symbol fonts are in the fallback list
        let names: Vec<&str> = FALLBACK_FAMILIES.to_vec();
        assert!(
            names.contains(&"NotoSansSymbols"),
            "should contain NotoSansSymbols"
        );
        assert!(
            names.contains(&"NotoSansSymbols2"),
            "should contain NotoSansSymbols2"
        );
        assert!(names.contains(&"DejaVuSans"), "should contain DejaVuSans");
    }

    #[test]
    fn test_scan_fallback_fonts_no_panic() {
        // scan_fallback_fonts should never panic, even if no fonts are installed
        let fonts = scan_fallback_fonts(16.0);
        // We can't assert a specific count because it depends on the system,
        // but it should return a Vec without panicking
        let _ = fonts.len();
    }

    #[test]
    fn test_scan_fallback_dir_nonexistent() {
        let families = vec![normalize_font_name("NotoSans")];
        let mut result = Vec::new();
        let nonexistent = std::path::PathBuf::from("/tmp/hs_test_nonexistent_fallback_99999");
        scan_fallback_dir(&nonexistent, &families, 16.0, &mut result);
        assert!(result.is_empty(), "nonexistent dir should yield no fonts");
    }

    #[test]
    fn test_scan_fallback_dir_skips_bold() {
        // Fallback scan should only accept Regular style, not Bold
        let dir = std::env::temp_dir().join("hs_test_fallback_bold_skip");
        let _ = std::fs::create_dir_all(&dir);
        let ttf = dir.join("NotoSans-Bold.ttf");
        std::fs::write(&ttf, b"not a real font").expect("write fake ttf");

        let families = vec![normalize_font_name("NotoSans")];
        let mut result = Vec::new();
        scan_fallback_dir(&dir, &families, 16.0, &mut result);

        // Even though the name matches, Bold style is rejected.
        // (Also, the data is fake so load_font fails too.)
        assert!(result.is_empty(), "Bold fonts should be skipped");

        let _ = std::fs::remove_file(&ttf);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_scan_fallback_dir_skips_non_matching() {
        let dir = std::env::temp_dir().join("hs_test_fallback_nomatch");
        let _ = std::fs::create_dir_all(&dir);
        let ttf = dir.join("ComicSans-Regular.ttf");
        std::fs::write(&ttf, b"not a font").expect("write");

        let families = vec![normalize_font_name("NotoSans")];
        let mut result = Vec::new();
        scan_fallback_dir(&dir, &families, 16.0, &mut result);

        assert!(result.is_empty(), "non-matching family should be skipped");

        let _ = std::fs::remove_file(&ttf);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_new_with_family_chains_include_fallbacks() {
        // The font chains should be longer when fallback fonts exist on the system.
        // At minimum: embedded font + emoji = 2 entries per chain.
        // With fallbacks: 2 + N where N >= 0.
        let fm = FontManager::new_with_family(16.0, None);
        let chain_len = fm.fonts[0].len();
        // Embedded regular + embedded emoji = at least 2
        assert!(
            chain_len >= 2,
            "regular chain should have at least embedded + emoji, got {chain_len}"
        );
    }

    #[test]
    fn test_rasterize_unicode_symbols_no_panic() {
        // These are the exact symbols that were blank in the Claude Code UI.
        // Even if the system lacks fallback fonts, rasterize must not panic.
        let mut fm = FontManager::new_with_family(16.0, None);
        let symbols = [
            "\u{2611}", // ☑ BALLOT BOX WITH CHECK
            "\u{2605}", // ★ BLACK STAR
            "\u{2713}", // ✓ CHECK MARK
            "\u{25CF}", // ● BLACK CIRCLE
            "\u{25CB}", // ○ WHITE CIRCLE
            "\u{25A0}", // ■ BLACK SQUARE
            "\u{25B6}", // ▶ BLACK RIGHT-POINTING TRIANGLE
            "\u{2022}", // • BULLET
            "\u{2039}", // ‹ SINGLE LEFT-POINTING ANGLE QUOTATION MARK
            "\u{203A}", // › SINGLE RIGHT-POINTING ANGLE QUOTATION MARK
        ];
        for sym in &symbols {
            let _ = fm.rasterize(sym, false, false);
        }
    }

    #[test]
    fn test_rasterize_unicode_symbols_with_styles() {
        let mut fm = FontManager::new_with_family(16.0, None);
        // Verify symbol rasterization doesn't panic in bold/italic variants
        let _ = fm.rasterize("\u{2611}", true, false); // bold ☑
        let _ = fm.rasterize("\u{2605}", false, true); // italic ★
        let _ = fm.rasterize("\u{2713}", true, true); // bold italic ✓
    }

    #[test]
    fn test_fallback_chain_all_styles_same_length() {
        // All four style chains should have the same number of fallback fonts
        // (fallbacks are Regular-only but appended to every chain).
        let fm = FontManager::new_with_family(16.0, None);
        let len0 = fm.fonts[0].len();
        for (i, chain) in fm.fonts.iter().enumerate() {
            assert_eq!(
                chain.len(),
                len0,
                "chain[{i}] length {} differs from chain[0] length {len0}",
                chain.len()
            );
        }
    }

    #[test]
    fn test_scan_fallback_dir_recurses_subdirs() {
        let dir = std::env::temp_dir().join("hs_test_fallback_recurse");
        let subdir = dir.join("noto");
        let _ = std::fs::create_dir_all(&subdir);
        let ttf = subdir.join("NotoSans-Regular.ttf");
        std::fs::write(&ttf, b"fake font data").expect("write");

        let families = vec![normalize_font_name("NotoSans")];
        let mut result = Vec::new();
        scan_fallback_dir(&dir, &families, 16.0, &mut result);

        // load_font fails on fake data, so no fonts loaded,
        // but the recursive path is exercised without panic.
        let _ = std::fs::remove_file(&ttf);
        let _ = std::fs::remove_dir(&subdir);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_rasterize_claude_code_ui_symbols() {
        // Regression test: these specific symbols appeared blank in Claude Code UI.
        // With fallback fonts installed, they should rasterize to Some.
        // Without, they may return None but must never panic.
        let mut fm = FontManager::new_with_family(16.0, None);

        // Star idle marker
        let star = fm
            .rasterize("\u{2605}", false, false)
            .map(|g| (g.width, g.height));

        // Filled checkbox
        let checkbox = fm
            .rasterize("\u{2611}", false, false)
            .map(|g| (g.width, g.height));

        // If we got glyphs, verify they have positive dimensions
        if let Some((w, h)) = star {
            assert!(w > 0 && h > 0, "star glyph should have positive size");
        }
        if let Some((w, h)) = checkbox {
            assert!(w > 0 && h > 0, "checkbox glyph should have positive size");
        }
    }
}
