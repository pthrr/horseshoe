//! Procedural rendering of box drawing (U+2500–U+257F), block elements (U+2580–U+259F),
//! and powerline glyphs (U+E0B0–U+E0B3). Drawn pixel-perfect at any cell size.

/// Check if a codepoint should be drawn procedurally.
pub const fn is_box_drawing(cp: u32) -> bool {
    matches!(cp, 0x2500..=0x257F | 0x2580..=0x259F | 0xE0B0..=0xE0B3)
}

/// Draw a box/block character into a caller-provided alpha buffer.
/// `buf` must be at least `cell_w * cell_h` bytes; it is zeroed before drawing.
/// Returns `true` if the codepoint was handled, `false` otherwise.
pub fn draw_box_char_into(cp: u32, cell_w: u32, cell_h: u32, buf: &mut [u8]) -> bool {
    let w = cell_w as usize;
    let h = cell_h as usize;
    let needed = w * h;
    if w == 0 || h == 0 || buf.len() < needed {
        return false;
    }

    let Some(alpha) = buf.get_mut(..needed) else {
        return false;
    };
    alpha.fill(0);

    match cp {
        0x2580..=0x259F => {
            draw_block_element(alpha, cp, w, h);
            true
        }
        0x2500..=0x257F => {
            draw_box_drawing(alpha, cp, w, h);
            true
        }
        0xE0B0..=0xE0B3 => {
            draw_powerline(alpha, cp, w, h);
            true
        }
        _ => false,
    }
}

/// Draw a box/block character into a cell-sized alpha buffer.
/// Returns a Vec<u8> of `cell_w * cell_h` alpha values (255 = fully opaque).
/// Returns `None` if the codepoint is not handled.
pub fn draw_box_char(cp: u32, cell_w: u32, cell_h: u32) -> Option<Vec<u8>> {
    let w = cell_w as usize;
    let h = cell_h as usize;
    if w == 0 || h == 0 {
        return None;
    }

    let mut alpha = vec![0u8; w * h];
    if draw_box_char_into(cp, cell_w, cell_h, &mut alpha) {
        Some(alpha)
    } else {
        None
    }
}

#[inline]
fn fill_rect(alpha: &mut [u8], w: usize, x: usize, y: usize, rw: usize, rh: usize) {
    for dy in 0..rh {
        let start = (y + dy) * w + x;
        if let Some(row) = alpha.get_mut(start..start + rw) {
            row.fill(255);
        }
    }
}

fn draw_block_element(alpha: &mut [u8], cp: u32, w: usize, h: usize) {
    match cp {
        // U+2580 UPPER HALF BLOCK
        0x2580 => fill_rect(alpha, w, 0, 0, w, h / 2),
        // U+2581 LOWER ONE EIGHTH BLOCK
        0x2581 => fill_rect(alpha, w, 0, h - h / 8, w, h / 8),
        // U+2582 LOWER ONE QUARTER BLOCK
        0x2582 => fill_rect(alpha, w, 0, h - h / 4, w, h / 4),
        // U+2583 LOWER THREE EIGHTHS BLOCK
        0x2583 => fill_rect(alpha, w, 0, h - h * 3 / 8, w, h * 3 / 8),
        // U+2584 LOWER HALF BLOCK
        0x2584 => fill_rect(alpha, w, 0, h / 2, w, h - h / 2),
        // U+2585 LOWER FIVE EIGHTHS BLOCK
        0x2585 => fill_rect(alpha, w, 0, h - h * 5 / 8, w, h * 5 / 8),
        // U+2586 LOWER THREE QUARTERS BLOCK
        0x2586 => fill_rect(alpha, w, 0, h - h * 3 / 4, w, h * 3 / 4),
        // U+2587 LOWER SEVEN EIGHTHS BLOCK
        0x2587 => fill_rect(alpha, w, 0, h - h * 7 / 8, w, h * 7 / 8),
        // U+2588 FULL BLOCK
        0x2588 => fill_rect(alpha, w, 0, 0, w, h),
        // U+2589 LEFT SEVEN EIGHTHS BLOCK
        0x2589 => fill_rect(alpha, w, 0, 0, w * 7 / 8, h),
        // U+258A LEFT THREE QUARTERS BLOCK
        0x258A => fill_rect(alpha, w, 0, 0, w * 3 / 4, h),
        // U+258B LEFT FIVE EIGHTHS BLOCK
        0x258B => fill_rect(alpha, w, 0, 0, w * 5 / 8, h),
        // U+258C LEFT HALF BLOCK
        0x258C => fill_rect(alpha, w, 0, 0, w / 2, h),
        // U+258D LEFT THREE EIGHTHS BLOCK
        0x258D => fill_rect(alpha, w, 0, 0, w * 3 / 8, h),
        // U+258E LEFT ONE QUARTER BLOCK
        0x258E => fill_rect(alpha, w, 0, 0, w / 4, h),
        // U+258F LEFT ONE EIGHTH BLOCK
        0x258F => fill_rect(alpha, w, 0, 0, w / 8, h),
        // U+2590 RIGHT HALF BLOCK
        0x2590 => fill_rect(alpha, w, w / 2, 0, w - w / 2, h),
        // U+2591 LIGHT SHADE (25%)
        0x2591 => {
            for y in 0..h {
                for x in 0..w {
                    if (x + y) % 4 == 0
                        && let Some(p) = alpha.get_mut(y * w + x)
                    {
                        *p = 255;
                    }
                }
            }
        }
        // U+2592 MEDIUM SHADE (50%)
        0x2592 => {
            for y in 0..h {
                for x in 0..w {
                    if (x + y) % 2 == 0
                        && let Some(p) = alpha.get_mut(y * w + x)
                    {
                        *p = 255;
                    }
                }
            }
        }
        // U+2593 DARK SHADE (75%)
        0x2593 => {
            for y in 0..h {
                for x in 0..w {
                    if (x + y) % 4 != 0
                        && let Some(p) = alpha.get_mut(y * w + x)
                    {
                        *p = 255;
                    }
                }
            }
        }
        // U+2594 UPPER ONE EIGHTH BLOCK
        0x2594 => fill_rect(alpha, w, 0, 0, w, h / 8),
        // U+2595 RIGHT ONE EIGHTH BLOCK
        0x2595 => fill_rect(alpha, w, w - w / 8, 0, w / 8, h),
        // Quadrants U+2596-U+259F
        0x2596 => fill_rect(alpha, w, 0, h / 2, w / 2, h - h / 2), // Lower left
        0x2597 => fill_rect(alpha, w, w / 2, h / 2, w - w / 2, h - h / 2), // Lower right
        0x2598 => fill_rect(alpha, w, 0, 0, w / 2, h / 2),         // Upper left
        0x2599 => {
            // Upper left + lower left + lower right
            fill_rect(alpha, w, 0, 0, w / 2, h / 2);
            fill_rect(alpha, w, 0, h / 2, w, h - h / 2);
        }
        0x259A => {
            // Upper left + lower right
            fill_rect(alpha, w, 0, 0, w / 2, h / 2);
            fill_rect(alpha, w, w / 2, h / 2, w - w / 2, h - h / 2);
        }
        0x259B => {
            // Upper left + upper right + lower left
            fill_rect(alpha, w, 0, 0, w, h / 2);
            fill_rect(alpha, w, 0, h / 2, w / 2, h - h / 2);
        }
        0x259C => {
            // Upper left + upper right + lower right
            fill_rect(alpha, w, 0, 0, w, h / 2);
            fill_rect(alpha, w, w / 2, h / 2, w - w / 2, h - h / 2);
        }
        0x259D => fill_rect(alpha, w, w / 2, 0, w - w / 2, h / 2), // Upper right
        0x259E => {
            // Upper right + lower left
            fill_rect(alpha, w, w / 2, 0, w - w / 2, h / 2);
            fill_rect(alpha, w, 0, h / 2, w / 2, h - h / 2);
        }
        0x259F => {
            // Upper right + lower left + lower right
            fill_rect(alpha, w, w / 2, 0, w - w / 2, h / 2);
            fill_rect(alpha, w, 0, h / 2, w, h - h / 2);
        }
        _ => {}
    }
}

/// Draw box drawing characters U+2500–U+257F.
/// Uses segments from cell edges to center for composability.
fn draw_box_drawing(alpha: &mut [u8], cp: u32, w: usize, h: usize) {
    let cx = w / 2;
    let cy = h / 2;
    let widths = LineWidths {
        thin: 1,
        thick: (w / 4).max(2),
    };

    // Get the line configuration for each direction:
    // (right, down, left, up) each can be: 0=none, 1=light, 2=heavy, 3=double
    let segments = box_segments(cp);
    if segments == (0, 0, 0, 0) {
        return;
    }

    let (right, down, left, up) = segments;

    // Draw each segment
    // Right: from center to right edge
    draw_h_segment(alpha, w, cx, cy, w, right, &widths);
    // Left: from left edge to center
    draw_h_segment(alpha, w, 0, cy, cx + 1, left, &widths);
    // Down: from center to bottom edge
    draw_v_segment(alpha, w, cx, cy, h, down, &widths);
    // Up: from top to center
    draw_v_segment(alpha, w, cx, 0, cy + 1, up, &widths);
}

/// Line thickness parameters for box drawing segments.
struct LineWidths {
    thin: usize,
    thick: usize,
}

/// Draw a horizontal segment in the alpha buffer.
fn draw_h_segment(
    alpha: &mut [u8],
    w: usize,
    x1: usize,
    cy: usize,
    x2: usize,
    style: u8,
    widths: &LineWidths,
) {
    match style {
        1 => {
            // Light line
            let y_start = cy;
            for x in x1..x2 {
                for dy in 0..widths.thin {
                    if let Some(p) = alpha.get_mut((y_start + dy) * w + x) {
                        *p = 255;
                    }
                }
            }
        }
        2 => {
            // Heavy line
            let y_start = cy.saturating_sub(widths.thick / 2);
            for x in x1..x2 {
                for dy in 0..widths.thick {
                    if let Some(p) = alpha.get_mut((y_start + dy) * w + x) {
                        *p = 255;
                    }
                }
            }
        }
        3 => {
            // Double line
            let gap = (widths.thick).max(2);
            let top = cy.saturating_sub(gap / 2 + widths.thin);
            let bot = cy + gap / 2;
            for x in x1..x2 {
                for dy in 0..widths.thin {
                    if let Some(p) = alpha.get_mut((top + dy) * w + x) {
                        *p = 255;
                    }
                    if let Some(p) = alpha.get_mut((bot + dy) * w + x) {
                        *p = 255;
                    }
                }
            }
        }
        _ => {}
    }
}

/// Draw a vertical segment in the alpha buffer.
fn draw_v_segment(
    alpha: &mut [u8],
    w: usize,
    cx: usize,
    y1: usize,
    y2: usize,
    style: u8,
    widths: &LineWidths,
) {
    match style {
        1 => {
            let x_start = cx;
            for y in y1..y2 {
                for dx in 0..widths.thin {
                    if let Some(p) = alpha.get_mut(y * w + x_start + dx) {
                        *p = 255;
                    }
                }
            }
        }
        2 => {
            let x_start = cx.saturating_sub(widths.thick / 2);
            for y in y1..y2 {
                for dx in 0..widths.thick {
                    if let Some(p) = alpha.get_mut(y * w + x_start + dx) {
                        *p = 255;
                    }
                }
            }
        }
        3 => {
            let gap = (widths.thick).max(2);
            let left = cx.saturating_sub(gap / 2 + widths.thin);
            let right = cx + gap / 2;
            for y in y1..y2 {
                for dx in 0..widths.thin {
                    if let Some(p) = alpha.get_mut(y * w + left + dx) {
                        *p = 255;
                    }
                    if let Some(p) = alpha.get_mut(y * w + right + dx) {
                        *p = 255;
                    }
                }
            }
        }
        _ => {}
    }
}

/// Return (right, down, left, up) segment styles for a box drawing codepoint.
/// 0=none, 1=light, 2=heavy, 3=double
const fn box_segments(cp: u32) -> (u8, u8, u8, u8) {
    // (right, down, left, up)
    match cp {
        // Light/heavy horizontal + dashes (same rendering)
        0x2500 | 0x2504 | 0x2508 | 0x254C => (1, 0, 1, 0), // ─ ┄ ┈ ╌
        0x2501 | 0x2505 | 0x2509 | 0x254D => (2, 0, 2, 0), // ━ ┅ ┉ ╍
        // Light/heavy vertical + dashes
        0x2502 | 0x2506 | 0x250A | 0x254E => (0, 1, 0, 1), // │ ┆ ┊ ╎
        0x2503 | 0x2507 | 0x250B | 0x254F => (0, 2, 0, 2), // ┃ ┇ ┋ ╏
        // Corners + rounded corners (same segments)
        0x250C | 0x256D => (1, 1, 0, 0), // ┌ ╭
        0x250D => (2, 1, 0, 0),          // ┍
        0x250E => (1, 2, 0, 0),          // ┎
        0x250F => (2, 2, 0, 0),          // ┏
        0x2510 | 0x256E => (0, 1, 1, 0), // ┐ ╮
        0x2511 => (0, 1, 2, 0),          // ┑
        0x2512 => (0, 2, 1, 0),          // ┒
        0x2513 => (0, 2, 2, 0),          // ┓
        0x2514 | 0x2570 => (1, 0, 0, 1), // └ ╰
        0x2515 => (2, 0, 0, 1),          // ┕
        0x2516 => (1, 0, 0, 2),          // ┖
        0x2517 => (2, 0, 0, 2),          // ┗
        0x2518 | 0x256F => (0, 0, 1, 1), // ┘ ╯
        0x2519 => (0, 0, 2, 1),          // ┙
        0x251A => (0, 0, 1, 2),          // ┚
        0x251B => (0, 0, 2, 2),          // ┛
        // T-pieces + cross pieces + double/half lines
        0x251C..=0x254B => box_segments_junctions(cp),
        0x2550..=0x256C => box_segments_double(cp),
        0x2574..=0x257F => box_segments_half(cp),
        _ => (0, 0, 0, 0),
    }
}

/// T-pieces and cross pieces (U+251C..U+254B).
const fn box_segments_junctions(cp: u32) -> (u8, u8, u8, u8) {
    match cp {
        // T-pieces (left)
        0x251C => (1, 1, 0, 1),
        0x251D => (2, 1, 0, 1),
        0x251E => (1, 2, 0, 1),
        0x251F => (1, 1, 0, 2),
        0x2520 => (1, 2, 0, 2),
        0x2521 => (2, 1, 0, 2),
        0x2522 => (2, 2, 0, 1),
        0x2523 => (2, 2, 0, 2),
        // T-pieces (right)
        0x2524 => (0, 1, 1, 1),
        0x2525 => (0, 1, 2, 1),
        0x2526 => (0, 2, 1, 1),
        0x2527 => (0, 1, 1, 2),
        0x2528 => (0, 2, 1, 2),
        0x2529 => (0, 1, 2, 2),
        0x252A => (0, 2, 2, 1),
        0x252B => (0, 2, 2, 2),
        // T-pieces (top)
        0x252C => (1, 1, 1, 0),
        0x252D => (1, 1, 2, 0),
        0x252E => (2, 1, 1, 0),
        0x252F => (2, 1, 2, 0),
        0x2530 => (1, 2, 1, 0),
        0x2531 => (1, 2, 2, 0),
        0x2532 => (2, 2, 1, 0),
        0x2533 => (2, 2, 2, 0),
        // T-pieces (bottom)
        0x2534 => (1, 0, 1, 1),
        0x2535 => (1, 0, 2, 1),
        0x2536 => (2, 0, 1, 1),
        0x2537 => (2, 0, 2, 1),
        0x2538 => (1, 0, 1, 2),
        0x2539 => (1, 0, 2, 2),
        0x253A => (2, 0, 1, 2),
        0x253B => (2, 0, 2, 2),
        // Cross pieces
        0x253C => (1, 1, 1, 1),
        0x253D => (1, 1, 2, 1),
        0x253E => (2, 1, 1, 1),
        0x253F => (2, 1, 2, 1),
        0x2540 => (1, 2, 1, 1),
        0x2541 => (1, 1, 1, 2),
        0x2542 => (1, 2, 1, 2),
        0x2543 => (1, 2, 2, 1),
        0x2544 => (2, 2, 1, 1),
        0x2545 => (1, 1, 2, 2),
        0x2546 => (2, 1, 1, 2),
        0x2547 => (2, 2, 2, 1),
        0x2548 => (2, 1, 2, 2),
        0x2549 => (1, 2, 2, 2),
        0x254A => (2, 2, 1, 2),
        0x254B => (2, 2, 2, 2),
        _ => (0, 0, 0, 0),
    }
}

/// Double-line characters (U+2550..U+256C).
const fn box_segments_double(cp: u32) -> (u8, u8, u8, u8) {
    match cp {
        0x2550 => (3, 0, 3, 0),
        0x2551 => (0, 3, 0, 3),
        0x2552 => (3, 1, 0, 0),
        0x2553 => (1, 3, 0, 0),
        0x2554 => (3, 3, 0, 0),
        0x2555 => (0, 1, 3, 0),
        0x2556 => (0, 3, 1, 0),
        0x2557 => (0, 3, 3, 0),
        0x2558 => (3, 0, 0, 1),
        0x2559 => (1, 0, 0, 3),
        0x255A => (3, 0, 0, 3),
        0x255B => (0, 0, 3, 1),
        0x255C => (0, 0, 1, 3),
        0x255D => (0, 0, 3, 3),
        0x255E => (3, 1, 0, 1),
        0x255F => (1, 3, 0, 3),
        0x2560 => (3, 3, 0, 3),
        0x2561 => (0, 1, 3, 1),
        0x2562 => (0, 3, 1, 3),
        0x2563 => (0, 3, 3, 3),
        0x2564 => (3, 1, 3, 0),
        0x2565 => (1, 3, 1, 0),
        0x2566 => (3, 3, 3, 0),
        0x2567 => (3, 0, 3, 1),
        0x2568 => (1, 0, 1, 3),
        0x2569 => (3, 0, 3, 3),
        0x256A => (3, 1, 3, 1),
        0x256B => (1, 3, 1, 3),
        0x256C => (3, 3, 3, 3),
        _ => (0, 0, 0, 0),
    }
}

/// Half and mixed-weight half lines (U+2574..U+257F).
const fn box_segments_half(cp: u32) -> (u8, u8, u8, u8) {
    match cp {
        0x2574 => (0, 0, 1, 0),
        0x2575 => (0, 0, 0, 1),
        0x2576 => (1, 0, 0, 0),
        0x2577 => (0, 1, 0, 0),
        0x2578 => (0, 0, 2, 0),
        0x2579 => (0, 0, 0, 2),
        0x257A => (2, 0, 0, 0),
        0x257B => (0, 2, 0, 0),
        0x257C => (2, 0, 1, 0),
        0x257D => (0, 2, 0, 1),
        0x257E => (1, 0, 2, 0),
        0x257F => (0, 1, 0, 2),
        _ => (0, 0, 0, 0),
    }
}

fn draw_powerline(alpha: &mut [u8], cp: u32, w: usize, h: usize) {
    match cp {
        // U+E0B0: Right-pointing solid triangle
        0xE0B0 => {
            for y in 0..h {
                // Width of fill at this scanline: proportional
                let fill_w = if h > 1 {
                    let half = h / 2;
                    if y <= half {
                        (y * w) / half
                    } else {
                        ((h - 1 - y) * w) / half
                    }
                } else {
                    w
                };
                for x in 0..fill_w {
                    if let Some(p) = alpha.get_mut(y * w + x) {
                        *p = 255;
                    }
                }
            }
        }
        // U+E0B1: Right-pointing outline triangle
        0xE0B1 => {
            for y in 0..h {
                let half = h / 2;
                let raw_edge = if h > 1 {
                    if y <= half {
                        (y * w) / half
                    } else {
                        ((h - 1 - y) * w) / half
                    }
                } else {
                    w
                };
                let clamped_edge = raw_edge.min(w.saturating_sub(1));
                if let Some(p) = alpha.get_mut(y * w + clamped_edge) {
                    *p = 255;
                }
            }
        }
        // U+E0B2: Left-pointing solid triangle
        0xE0B2 => {
            for y in 0..h {
                let half = h / 2;
                let fill_w = if h > 1 {
                    if y <= half {
                        (y * w) / half
                    } else {
                        ((h - 1 - y) * w) / half
                    }
                } else {
                    w
                };
                for x in 0..fill_w {
                    if let Some(p) = alpha.get_mut(y * w + (w - 1 - x)) {
                        *p = 255;
                    }
                }
            }
        }
        // U+E0B3: Left-pointing outline triangle
        0xE0B3 => {
            for y in 0..h {
                let half = h / 2;
                let edge_x = if h > 1 {
                    if y <= half {
                        (y * w) / half
                    } else {
                        ((h - 1 - y) * w) / half
                    }
                } else {
                    0
                };
                let px = w
                    .saturating_sub(1)
                    .saturating_sub(edge_x.min(w.saturating_sub(1)));
                if let Some(p) = alpha.get_mut(y * w + px) {
                    *p = 255;
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: get pixel value from alpha buffer at (col, row) with bounds checking.
    fn pixel(alpha: &[u8], stride: usize, col: usize, row: usize) -> u8 {
        *alpha
            .get(row * stride + col)
            .expect("pixel index in bounds")
    }

    /// Helper: count filled (255) bytes in a slice using `bytecount`.
    fn count_filled(data: &[u8]) -> usize {
        bytecount::count(data, 255)
    }

    #[test]
    fn test_is_box_drawing() {
        assert!(is_box_drawing(0x2500)); // ─
        assert!(is_box_drawing(0x2588)); // █
        assert!(is_box_drawing(0xE0B0)); // powerline
        assert!(!is_box_drawing(0x0041)); // 'A'
        assert!(!is_box_drawing(0x0020)); // space
    }

    #[test]
    fn test_full_block() {
        let alpha = draw_box_char(0x2588, 8, 16).expect("should handle full block");
        assert_eq!(alpha.len(), 8 * 16);
        // Every pixel should be filled
        assert!(alpha.iter().all(|&val| val == 255));
    }

    #[test]
    fn test_upper_half_block() {
        let alpha = draw_box_char(0x2580, 8, 16).expect("should handle upper half");
        assert_eq!(alpha.len(), 8 * 16);
        // Upper half should be filled
        for row in 0..8 {
            for col in 0..8 {
                assert_eq!(
                    pixel(&alpha, 8, col, row),
                    255,
                    "pixel at ({col},{row}) should be filled"
                );
            }
        }
        // Lower half should be empty
        for row in 8..16 {
            for col in 0..8 {
                assert_eq!(
                    pixel(&alpha, 8, col, row),
                    0,
                    "pixel at ({col},{row}) should be empty"
                );
            }
        }
    }

    #[test]
    fn test_lower_half_block() {
        let alpha = draw_box_char(0x2584, 8, 16).expect("should handle lower half");
        // Upper half empty, lower half filled
        for row in 0..8 {
            for col in 0..8 {
                assert_eq!(pixel(&alpha, 8, col, row), 0);
            }
        }
        for row in 8..16 {
            for col in 0..8 {
                assert_eq!(pixel(&alpha, 8, col, row), 255);
            }
        }
    }

    #[test]
    fn test_left_half_block() {
        let alpha = draw_box_char(0x258C, 8, 16).expect("should handle left half");
        for row in 0..16 {
            for col in 0..4 {
                assert_eq!(
                    pixel(&alpha, 8, col, row),
                    255,
                    "left half pixel ({col},{row})"
                );
            }
            for col in 4..8 {
                assert_eq!(
                    pixel(&alpha, 8, col, row),
                    0,
                    "right half pixel ({col},{row})"
                );
            }
        }
    }

    #[test]
    fn test_horizontal_line() {
        let alpha = draw_box_char(0x2500, 8, 16).expect("should handle ─");
        // Center row should have pixels set
        let center_row = 8; // cell_h / 2
        let mut has_center_pixels = false;
        for col in 0..8 {
            if pixel(&alpha, 8, col, center_row) == 255 {
                has_center_pixels = true;
            }
        }
        assert!(
            has_center_pixels,
            "horizontal line should have pixels at center row"
        );
    }

    #[test]
    fn test_vertical_line() {
        let alpha = draw_box_char(0x2502, 8, 16).expect("should handle │");
        // Center column should have pixels set top to bottom
        let center_col = 4; // cell_w / 2
        let mut filled_rows = 0;
        for row in 0..16 {
            if pixel(&alpha, 8, center_col, row) == 255 {
                filled_rows += 1;
            }
        }
        assert!(
            filled_rows >= 14,
            "vertical line should span most of cell height, got {filled_rows}"
        );
    }

    #[test]
    fn test_cross() {
        let alpha = draw_box_char(0x253C, 8, 16).expect("should handle ┼");
        // Should have both horizontal and vertical pixels
        let center_col = 4;
        let center_row = 8;
        let mut horiz_count = 0;
        let mut vert_count = 0;
        for col in 0..8 {
            if pixel(&alpha, 8, col, center_row) == 255 {
                horiz_count += 1;
            }
        }
        for row in 0..16 {
            if pixel(&alpha, 8, center_col, row) == 255 {
                vert_count += 1;
            }
        }
        assert!(horiz_count >= 6, "cross should have horizontal pixels");
        assert!(vert_count >= 14, "cross should have vertical pixels");
    }

    #[test]
    fn test_powerline_solid_right() {
        let alpha = draw_box_char(0xE0B0, 8, 16).expect("should handle powerline right");
        assert_eq!(alpha.len(), 8 * 16);
        // Top-left should be empty (or very narrow), middle rows wider
        let mid_row = 8;
        let mut mid_filled = 0;
        for col in 0..8 {
            if pixel(&alpha, 8, col, mid_row) == 255 {
                mid_filled += 1;
            }
        }
        assert!(mid_filled > 0, "powerline middle should have filled pixels");
    }

    #[test]
    fn test_medium_shade() {
        let alpha = draw_box_char(0x2592, 8, 16).expect("should handle medium shade");
        let filled = count_filled(&alpha);
        let total = 8 * 16;
        // Should be roughly 50%
        assert!(filled > total / 4, "medium shade should have >25% filled");
        assert!(
            filled < total * 3 / 4,
            "medium shade should have <75% filled"
        );
    }

    #[test]
    fn test_unhandled_returns_none() {
        assert!(draw_box_char(0x0041, 8, 16).is_none()); // 'A'
    }

    #[test]
    fn test_zero_dimensions() {
        assert!(draw_box_char(0x2500, 0, 16).is_none());
        assert!(draw_box_char(0x2500, 8, 0).is_none());
    }

    #[test]
    fn test_double_horizontal() {
        let alpha = draw_box_char(0x2550, 8, 16).expect("should handle ═");
        // Should have two horizontal lines
        let center_row: usize = 8;
        // Check rows above and below center for double line
        let mut line_rows = 0;
        for row in center_row.saturating_sub(4)..=(center_row + 4).min(15) {
            let mut has_pixels = false;
            for col in 0..8 {
                if pixel(&alpha, 8, col, row) == 255 {
                    has_pixels = true;
                    break;
                }
            }
            if has_pixels {
                line_rows += 1;
            }
        }
        assert!(
            line_rows >= 2,
            "double line should have at least 2 rows of pixels, got {line_rows}"
        );
    }

    #[test]
    fn test_right_half_block() {
        let alpha = draw_box_char(0x2590, 8, 16).expect("should handle right half block");
        for row in 0..16 {
            // Left half should be empty
            for col in 0..4 {
                assert_eq!(
                    pixel(&alpha, 8, col, row),
                    0,
                    "left side ({col},{row}) should be empty"
                );
            }
            // Right half should be filled
            for col in 4..8 {
                assert_eq!(
                    pixel(&alpha, 8, col, row),
                    255,
                    "right side ({col},{row}) should be filled"
                );
            }
        }
    }

    #[test]
    fn test_quadrant_lower_left() {
        let alpha = draw_box_char(0x2596, 8, 16).expect("lower left quadrant");
        // Upper half empty
        for row in 0..8 {
            for col in 0..8 {
                assert_eq!(pixel(&alpha, 8, col, row), 0, "upper half at ({col},{row})");
            }
        }
        // Lower left filled
        for row in 8..16 {
            for col in 0..4 {
                assert_eq!(
                    pixel(&alpha, 8, col, row),
                    255,
                    "lower left at ({col},{row})"
                );
            }
        }
    }

    #[test]
    fn test_quadrant_lower_right() {
        let alpha = draw_box_char(0x2597, 8, 16).expect("lower right quadrant");
        for row in 8..16 {
            for col in 4..8 {
                assert_eq!(
                    pixel(&alpha, 8, col, row),
                    255,
                    "lower right at ({col},{row})"
                );
            }
        }
        // Upper left empty
        assert_eq!(pixel(&alpha, 8, 0, 0), 0);
    }

    #[test]
    fn test_quadrant_upper_left() {
        let alpha = draw_box_char(0x2598, 8, 16).expect("upper left quadrant");
        for row in 0..8 {
            for col in 0..4 {
                assert_eq!(
                    pixel(&alpha, 8, col, row),
                    255,
                    "upper left at ({col},{row})"
                );
            }
        }
        // Lower right empty
        for row in 8..16 {
            for col in 4..8 {
                assert_eq!(
                    pixel(&alpha, 8, col, row),
                    0,
                    "lower right at ({col},{row})"
                );
            }
        }
    }

    #[test]
    fn test_quadrant_upper_right() {
        let alpha = draw_box_char(0x259D, 8, 16).expect("upper right quadrant");
        for row in 0..8 {
            for col in 4..8 {
                assert_eq!(
                    pixel(&alpha, 8, col, row),
                    255,
                    "upper right at ({col},{row})"
                );
            }
        }
    }

    #[test]
    fn test_quadrant_upper_left_lower_right() {
        let alpha = draw_box_char(0x259A, 8, 16).expect("UL + LR quadrant");
        // Upper left filled (row 0, col 0)
        assert_eq!(pixel(&alpha, 8, 0, 0), 255);
        // Lower right filled (row 15, col 7)
        assert_eq!(pixel(&alpha, 8, 7, 15), 255);
        // Upper right empty (row 0, col 7)
        assert_eq!(pixel(&alpha, 8, 7, 0), 0);
        // Lower left empty (row 15, col 0)
        assert_eq!(pixel(&alpha, 8, 0, 15), 0);
    }

    #[test]
    fn test_quadrant_upper_right_lower_left() {
        let alpha = draw_box_char(0x259E, 8, 16).expect("UR + LL quadrant");
        // Upper right filled (row 0, col 7)
        assert_eq!(pixel(&alpha, 8, 7, 0), 255);
        // Lower left filled (row 15, col 0)
        assert_eq!(pixel(&alpha, 8, 0, 15), 255);
    }

    #[test]
    fn test_quadrant_three_quarters_259b() {
        // UL + UR + LL
        let alpha = draw_box_char(0x259B, 8, 16).expect("UL+UR+LL quadrant");
        // Upper half fully filled
        for row in 0..8 {
            for col in 0..8 {
                assert_eq!(pixel(&alpha, 8, col, row), 255, "top at ({col},{row})");
            }
        }
        // Lower left filled
        for row in 8..16 {
            for col in 0..4 {
                assert_eq!(
                    pixel(&alpha, 8, col, row),
                    255,
                    "lower left at ({col},{row})"
                );
            }
        }
    }

    #[test]
    fn test_quadrant_259f() {
        // UR + LL + LR
        let alpha = draw_box_char(0x259F, 8, 16).expect("UR+LL+LR quadrant");
        // Upper right filled (row 0, col 7)
        assert_eq!(pixel(&alpha, 8, 7, 0), 255);
        // Lower half fully filled
        for row in 8..16 {
            for col in 0..8 {
                assert_eq!(pixel(&alpha, 8, col, row), 255, "lower at ({col},{row})");
            }
        }
    }

    #[test]
    fn test_light_shade() {
        let alpha = draw_box_char(0x2591, 8, 16).expect("light shade");
        let filled = count_filled(&alpha);
        let total = 8 * 16;
        // Light shade = ~25%
        assert!(filled > 0, "light shade should have some pixels");
        assert!(
            filled < total / 2,
            "light shade should have <50% filled, got {filled}"
        );
    }

    #[test]
    fn test_dark_shade() {
        let alpha = draw_box_char(0x2593, 8, 16).expect("dark shade");
        let filled = count_filled(&alpha);
        let total = 8 * 16;
        // Dark shade = ~75%
        assert!(
            filled > total / 2,
            "dark shade should have >50% filled, got {filled}"
        );
        assert!(filled < total, "dark shade should not be fully filled");
    }

    #[test]
    fn test_rounded_corner_top_left() {
        // 0x256D = ╭ (same segments as ┌)
        assert!(draw_box_char(0x256D, 8, 16).is_some());
        assert_eq!(box_segments(0x256D), (1, 1, 0, 0)); // right + down
    }

    #[test]
    fn test_rounded_corner_top_right() {
        assert!(draw_box_char(0x256E, 8, 16).is_some());
        assert_eq!(box_segments(0x256E), (0, 1, 1, 0)); // down + left
    }

    #[test]
    fn test_rounded_corner_bottom_right() {
        assert!(draw_box_char(0x256F, 8, 16).is_some());
        assert_eq!(box_segments(0x256F), (0, 0, 1, 1)); // left + up
    }

    #[test]
    fn test_rounded_corner_bottom_left() {
        assert!(draw_box_char(0x2570, 8, 16).is_some());
        assert_eq!(box_segments(0x2570), (1, 0, 0, 1)); // right + up
    }

    #[test]
    fn test_powerline_left_solid() {
        let alpha = draw_box_char(0xE0B2, 8, 16).expect("powerline left solid");
        assert_eq!(alpha.len(), 8 * 16);
        // Middle row should have some filled pixels
        let mid_row = 8;
        let mut mid_filled = 0;
        for col in 0..8 {
            if pixel(&alpha, 8, col, mid_row) == 255 {
                mid_filled += 1;
            }
        }
        assert!(
            mid_filled > 0,
            "left triangle middle should have filled pixels"
        );
    }

    #[test]
    fn test_powerline_outline_right() {
        let alpha = draw_box_char(0xE0B1, 8, 16).expect("powerline right outline");
        assert_eq!(alpha.len(), 8 * 16);
        // Should have exactly one pixel per row (outline)
        for row in 0..16 {
            let row_count: usize = (0..8)
                .filter(|&col| pixel(&alpha, 8, col, row) == 255)
                .count();
            assert!(
                row_count <= 2,
                "outline should have at most 2 pixels per row, got {row_count} at row {row}"
            );
        }
    }

    #[test]
    fn test_powerline_outline_left() {
        let alpha = draw_box_char(0xE0B3, 8, 16).expect("powerline left outline");
        assert_eq!(alpha.len(), 8 * 16);
        let total_filled = count_filled(&alpha);
        // Outline: one pixel per row = 16 pixels
        assert!(
            total_filled <= 32,
            "outline should be sparse, got {total_filled}"
        );
        assert!(total_filled > 0, "outline should have some pixels");
    }

    #[test]
    fn test_box_segments_light_horizontal() {
        assert_eq!(box_segments(0x2500), (1, 0, 1, 0));
    }

    #[test]
    fn test_box_segments_heavy_horizontal() {
        assert_eq!(box_segments(0x2501), (2, 0, 2, 0));
    }

    #[test]
    fn test_box_segments_light_vertical() {
        assert_eq!(box_segments(0x2502), (0, 1, 0, 1));
    }

    #[test]
    fn test_box_segments_heavy_vertical() {
        assert_eq!(box_segments(0x2503), (0, 2, 0, 2));
    }

    #[test]
    fn test_box_segments_double_lines() {
        assert_eq!(box_segments(0x2550), (3, 0, 3, 0)); // ═
        assert_eq!(box_segments(0x2551), (0, 3, 0, 3)); // ║
    }

    #[test]
    fn test_box_segments_cross() {
        assert_eq!(box_segments(0x253C), (1, 1, 1, 1)); // ┼
        assert_eq!(box_segments(0x254B), (2, 2, 2, 2)); // ╋
        assert_eq!(box_segments(0x256C), (3, 3, 3, 3)); // ╬
    }

    #[test]
    fn test_box_segments_unhandled() {
        assert_eq!(box_segments(0x0041), (0, 0, 0, 0)); // 'A'
        assert_eq!(box_segments(0xFFFF), (0, 0, 0, 0));
    }

    #[test]
    fn test_box_segments_half_lines() {
        assert_eq!(box_segments(0x2574), (0, 0, 1, 0)); // left half
        assert_eq!(box_segments(0x2576), (1, 0, 0, 0)); // right half
        assert_eq!(box_segments(0x2575), (0, 0, 0, 1)); // up half
        assert_eq!(box_segments(0x2577), (0, 1, 0, 0)); // down half
    }

    #[test]
    fn test_is_box_drawing_boundaries() {
        // Just below range
        assert!(!is_box_drawing(0x24FF));
        // First in range
        assert!(is_box_drawing(0x2500));
        // Last box drawing
        assert!(is_box_drawing(0x257F));
        // 0x257F is the last box drawing char (in range), so there's no gap
        // First block element
        assert!(is_box_drawing(0x2580));
        // Last block element
        assert!(is_box_drawing(0x259F));
        // Just past block elements
        assert!(!is_box_drawing(0x25A0));
        // Powerline range
        assert!(is_box_drawing(0xE0B0));
        assert!(is_box_drawing(0xE0B3));
        assert!(!is_box_drawing(0xE0B4));
        assert!(!is_box_drawing(0xE0AF));
    }

    #[test]
    fn test_lower_one_eighth_block() {
        let alpha = draw_box_char(0x2581, 8, 16).expect("lower 1/8");
        // Only the bottom 2 rows (16/8=2) should be filled
        let bottom_slice = alpha.get(14 * 8..).expect("bottom slice in bounds");
        let filled_bottom = count_filled(bottom_slice);
        assert!(filled_bottom > 0, "bottom should have pixels");
        // Top should be empty
        for row in 0..12 {
            for col in 0..8 {
                assert_eq!(
                    pixel(&alpha, 8, col, row),
                    0,
                    "top area should be empty at ({col},{row})"
                );
            }
        }
    }

    #[test]
    fn test_upper_one_eighth_block() {
        let alpha = draw_box_char(0x2594, 8, 16).expect("upper 1/8");
        // Only top 2 rows filled
        let top_slice = alpha.get(..2 * 8).expect("top slice in bounds");
        let filled_top = count_filled(top_slice);
        assert!(filled_top > 0, "top should have pixels");
    }

    #[test]
    fn test_right_one_eighth_block() {
        let alpha = draw_box_char(0x2595, 8, 16).expect("right 1/8");
        // Right 1 col should be filled (8/8=1)
        for row in 0..16 {
            assert_eq!(pixel(&alpha, 8, 7, row), 255, "rightmost col at row {row}");
        }
    }

    #[test]
    fn test_all_box_drawing_range_no_panic() {
        for cp in 0x2500..=0x257F {
            let result = draw_box_char(cp, 10, 20);
            assert!(result.is_some(), "box drawing U+{cp:04X} should be handled");
        }
        for cp in 0x2580..=0x259F {
            let result = draw_box_char(cp, 10, 20);
            assert!(
                result.is_some(),
                "block element U+{cp:04X} should be handled"
            );
        }
        for cp in 0xE0B0..=0xE0B3 {
            let result = draw_box_char(cp, 10, 20);
            assert!(result.is_some(), "powerline U+{cp:04X} should be handled");
        }
    }

    #[test]
    fn test_box_char_minimum_cell_size() {
        // 1x1 should not panic; some glyphs may produce empty alpha but shouldn't crash
        for cp in [0x2500, 0x2502, 0x253C, 0x2588, 0xE0B0] {
            let alpha = draw_box_char(cp, 1, 1)
                .unwrap_or_else(|| panic!("1x1 cell for U+{cp:04X} should return Some"));
            assert_eq!(alpha.len(), 1, "1x1 buffer should be exactly 1 byte");
        }
    }

    #[test]
    fn test_powerline_1px_width() {
        // Width=1 shouldn't panic for any powerline glyph
        for cp in 0xE0B0..=0xE0B3 {
            let alpha = draw_box_char(cp, 1, 16)
                .unwrap_or_else(|| panic!("1px-wide powerline U+{cp:04X} should return Some"));
            assert_eq!(alpha.len(), 16);
        }
    }

    #[test]
    fn test_all_powerline_produce_output() {
        for cp in 0xE0B0..=0xE0B3 {
            assert!(
                draw_box_char(cp, 8, 16).is_some(),
                "powerline U+{cp:04X} should produce output"
            );
        }
    }

    #[test]
    fn test_box_drawing_nonzero_alpha_all() {
        // Diagonal characters (0x2571-0x2573) are not drawn by the segment system,
        // so we exclude them from the non-zero pixel assertion.
        let diagonals = [0x2571u32, 0x2572, 0x2573];
        let ranges: &[std::ops::RangeInclusive<u32>] =
            &[0x2500..=0x257F, 0x2580..=0x259F, 0xE0B0..=0xE0B3];
        for range in ranges {
            for cp in range.clone() {
                let alpha = draw_box_char(cp, 8, 16)
                    .unwrap_or_else(|| panic!("U+{cp:04X} should be handled"));
                if diagonals.contains(&cp) {
                    continue; // skip unimplemented diagonals
                }
                let has_nonzero = alpha.iter().any(|&v| v != 0);
                assert!(
                    has_nonzero,
                    "U+{cp:04X} should have at least one non-zero alpha pixel at 8x16"
                );
            }
        }
    }

    #[test]
    fn test_large_cell_size() {
        for &cp in &[0x2500, 0x2588, 0x253C, 0xE0B0] {
            let alpha = draw_box_char(cp, 64, 128)
                .unwrap_or_else(|| panic!("64x128 cell for U+{cp:04X} should return Some"));
            assert_eq!(alpha.len(), 64 * 128);
        }
    }

    #[test]
    fn test_fill_rect_full_cell() {
        let mut alpha = vec![0u8; 8 * 16];
        fill_rect(&mut alpha, 8, 0, 0, 8, 16);
        assert!(alpha.iter().all(|&v| v == 255));
    }

    #[test]
    fn test_fill_rect_zero_dims() {
        let mut alpha = vec![0u8; 64];
        let original = alpha.clone();
        fill_rect(&mut alpha, 8, 0, 0, 0, 8); // zero width
        assert_eq!(alpha, original);
        fill_rect(&mut alpha, 8, 0, 0, 8, 0); // zero height
        assert_eq!(alpha, original);
    }

    #[test]
    fn test_fill_rect_partial_oob() {
        let mut alpha = vec![0u8; 16]; // 4x4
        // Try to fill a rect that goes beyond the buffer
        fill_rect(&mut alpha, 4, 2, 2, 4, 4);
        // Should not panic; only in-bounds rows filled
        // Row y=2: start=2*4+2=10, end=10+4=14 — fits (alpha is 16)
        // Row y=3: start=3*4+2=14, end=14+4=18 — does NOT fit (alpha is 16)
        // So row 2 filled, row 3 skipped
        assert_eq!(*alpha.get(10).expect("byte"), 255);
        assert_eq!(*alpha.get(11).expect("byte"), 255);
    }

    #[test]
    fn test_block_half_complement() {
        // Upper half + lower half should cover entire cell
        let upper = draw_box_char(0x2580, 8, 16).expect("upper half");
        let lower = draw_box_char(0x2584, 8, 16).expect("lower half");
        for i in 0..upper.len() {
            let u = *upper.get(i).unwrap_or(&0);
            let l = *lower.get(i).unwrap_or(&0);
            assert!(
                u == 255 || l == 255,
                "pixel {i} should be covered by either upper or lower half"
            );
        }
    }

    #[test]
    fn test_double_horizontal_two_rows() {
        let alpha = draw_box_char(0x2550, 8, 16).expect("═");
        // Count distinct rows that have at least one filled pixel
        let mut filled_rows = 0;
        for row in 0..16 {
            let has_pixel = (0..8).any(|col| *alpha.get(row * 8 + col).unwrap_or(&0) == 255);
            if has_pixel {
                filled_rows += 1;
            }
        }
        assert!(
            filled_rows >= 2,
            "double horizontal should have >=2 filled rows, got {filled_rows}"
        );
    }

    #[test]
    fn test_box_drawing_cross_center() {
        let alpha = draw_box_char(0x253C, 8, 16).expect("┼");
        let cx = 4; // cell_w / 2
        let cy = 8; // cell_h / 2
        assert_eq!(
            *alpha.get(cy * 8 + cx).unwrap_or(&0),
            255,
            "center pixel of cross should be filled"
        );
    }

    #[test]
    fn test_draw_box_char_into_prefilled_buffer() {
        let w = 8u32;
        let h = 16u32;
        let size = (w * h) as usize;
        let mut buf = vec![0xFFu8; size];
        // Draw full block (U+2588) into the prefilled buffer
        let handled = draw_box_char_into(0x2588, w, h, &mut buf);
        assert!(handled, "full block should be handled");
        // The function zeroes the buffer first, then draws.
        // For full block, every byte should be 0xFF (filled after zero).
        assert!(
            buf.iter().all(|&v| v == 255),
            "full block should fill all pixels to 255"
        );

        // Now draw a character that only partially fills (e.g., upper half block)
        let mut buf2 = vec![0xFFu8; size];
        let handled2 = draw_box_char_into(0x2580, w, h, &mut buf2);
        assert!(handled2, "upper half block should be handled");
        // The lower half should be zeroed (because the buffer was zeroed first)
        for row in (h / 2) as usize..h as usize {
            for col in 0..w as usize {
                let idx = row * w as usize + col;
                assert_eq!(
                    *buf2.get(idx).expect("pixel in bounds"),
                    0,
                    "lower half pixel ({col},{row}) should be 0 (buffer was zeroed before draw)"
                );
            }
        }
    }

    #[test]
    fn test_fill_rect_completely_out_of_bounds() {
        let w = 8usize;
        let h = 8usize;
        let mut alpha = vec![0u8; w * h];
        let original = alpha.clone();
        // y fully beyond the buffer: start index exceeds buffer length
        // y=h means start = h*w + x which is >= buf.len(), so get_mut returns None
        fill_rect(&mut alpha, w, 0, h, 4, 4);
        assert_eq!(
            alpha, original,
            "fill_rect with y >= h should not write anything"
        );
        // Both x and y way beyond buffer
        fill_rect(&mut alpha, w, 100, 100, 4, 4);
        assert_eq!(
            alpha, original,
            "fill_rect with x,y >> w,h should not write anything"
        );
        // Large y alone
        fill_rect(&mut alpha, w, 0, 1000, 4, 4);
        assert_eq!(
            alpha, original,
            "fill_rect with y >> h should not write anything"
        );
    }

    #[test]
    fn test_box_segments_all_box_drawing_range() {
        // Verify box_segments does not panic for any codepoint in the box drawing range
        for cp in 0x2500u32..=0x257F {
            let (right, down, left, up) = box_segments(cp);
            // Each segment should be 0..=3
            assert!(right <= 3, "right segment out of range for U+{cp:04X}");
            assert!(down <= 3, "down segment out of range for U+{cp:04X}");
            assert!(left <= 3, "left segment out of range for U+{cp:04X}");
            assert!(up <= 3, "up segment out of range for U+{cp:04X}");
        }
    }

    #[test]
    fn test_draw_box_char_2x2_cell() {
        // Very small cell (2x2) should not panic for any box drawing char
        for cp in 0x2500u32..=0x257F {
            let result = draw_box_char(cp, 2, 2);
            assert!(
                result.is_some(),
                "2x2 cell should produce Some for U+{cp:04X}"
            );
            let alpha = result.expect("just checked");
            assert_eq!(
                alpha.len(),
                4,
                "2x2 buffer should be 4 bytes for U+{cp:04X}"
            );
        }
        for cp in 0x2580u32..=0x259F {
            let result = draw_box_char(cp, 2, 2);
            assert!(
                result.is_some(),
                "2x2 cell should produce Some for block element U+{cp:04X}"
            );
        }
        for cp in 0xE0B0u32..=0xE0B3 {
            let result = draw_box_char(cp, 2, 2);
            assert!(
                result.is_some(),
                "2x2 cell should produce Some for powerline U+{cp:04X}"
            );
        }
    }

    #[test]
    fn test_powerline_mirror_symmetry() {
        // U+E0B0 (right-pointing solid triangle) and U+E0B2 (left-pointing solid triangle)
        // should be horizontal mirrors of each other.
        let w = 16u32;
        let h = 32u32;
        let right = draw_box_char(0xE0B0, w, h).expect("E0B0 should be handled");
        let left = draw_box_char(0xE0B2, w, h).expect("E0B2 should be handled");

        let ww = w as usize;
        let hh = h as usize;

        // For each row, the right-pointing triangle filled from the left should mirror
        // the left-pointing triangle filled from the right.
        for row in 0..hh {
            for col in 0..ww {
                let right_val = *right.get(row * ww + col).expect("right pixel");
                let left_val = *left.get(row * ww + (ww - 1 - col)).expect("left pixel");
                assert_eq!(
                    right_val,
                    left_val,
                    "mirror mismatch at row={row} col={col}: E0B0[{col}]={right_val} vs E0B2[{}]={left_val}",
                    ww - 1 - col
                );
            }
        }
    }

    #[test]
    fn test_block_element_full_block_all_ff() {
        let w = 10u32;
        let h = 20u32;
        let alpha = draw_box_char(0x2588, w, h).expect("full block should be handled");
        assert_eq!(alpha.len(), (w * h) as usize);
        for (idx, &val) in alpha.iter().enumerate() {
            assert_eq!(val, 0xFF, "full block pixel {idx} should be 0xFF");
        }
    }

    #[test]
    fn test_shade_percentage_approximate() {
        // U+2591 LIGHT SHADE uses pattern (x+y)%4==0, so approximately 25% of pixels
        let w = 32u32;
        let h = 32u32;
        let alpha = draw_box_char(0x2591, w, h).expect("light shade should be handled");
        let total = (w * h) as usize;
        let nonzero = alpha.iter().filter(|&&v| v != 0).count();
        // Should be approximately 25% (tolerance: 20%-30%)
        let low = total / 5; // 20%
        let high = total * 3 / 10; // 30%
        let pct = nonzero * 100 / total;
        assert!(
            nonzero >= low && nonzero <= high,
            "light shade should have ~25% non-zero pixels, got {nonzero}/{total} (~{pct}%)",
        );
    }

    #[test]
    fn test_draw_box_char_into_zero_width() {
        let mut buf = vec![0u8; 64];
        assert!(
            !draw_box_char_into(0x2500, 0, 10, &mut buf),
            "zero width should return false"
        );
    }

    #[test]
    fn test_draw_box_char_into_zero_height() {
        let mut buf = vec![0u8; 64];
        assert!(
            !draw_box_char_into(0x2500, 10, 0, &mut buf),
            "zero height should return false"
        );
    }

    #[test]
    fn test_draw_box_char_into_buffer_too_small() {
        // 8x16 needs 128 bytes; provide only 1 byte
        let mut buf = [0u8; 1];
        assert!(
            !draw_box_char_into(0x2500, 8, 16, &mut buf),
            "undersized buffer should return false"
        );
    }

    #[test]
    fn test_powerline_e0b0_height_1() {
        // U+E0B0 right-pointing solid triangle: h==1 triggers `else { w }` branch
        let mut buf = vec![0u8; 10];
        assert!(draw_box_char_into(0xE0B0, 10, 1, &mut buf));
        // With h==1, fill_w = w for the single row
        let filled = bytecount::count(&buf, 255);
        assert_eq!(
            filled, 10,
            "h=1 solid right triangle should fill entire width"
        );
    }

    #[test]
    fn test_powerline_e0b1_height_1() {
        // U+E0B1 right-pointing outline triangle: h==1 triggers `else { w }` branch
        let mut buf = vec![0u8; 10];
        assert!(draw_box_char_into(0xE0B1, 10, 1, &mut buf));
        // With h==1, raw_edge = w, clamped to w-1, so one pixel at column w-1
        let filled = bytecount::count(&buf, 255);
        assert!(
            filled >= 1,
            "h=1 outline right triangle should have at least 1 pixel"
        );
    }

    #[test]
    fn test_powerline_e0b2_height_1() {
        // U+E0B2 left-pointing solid triangle: h==1 triggers `else { w }` branch
        let mut buf = vec![0u8; 10];
        assert!(draw_box_char_into(0xE0B2, 10, 1, &mut buf));
        let filled = bytecount::count(&buf, 255);
        assert_eq!(
            filled, 10,
            "h=1 solid left triangle should fill entire width"
        );
    }

    #[test]
    fn test_powerline_e0b3_height_1() {
        // U+E0B3 left-pointing outline triangle: h==1 triggers `else { 0 }` branch
        let mut buf = vec![0u8; 10];
        assert!(draw_box_char_into(0xE0B3, 10, 1, &mut buf));
        // With h==1, edge_x = 0, px = w-1, so one pixel at column w-1
        let filled = bytecount::count(&buf, 255);
        assert!(
            filled >= 1,
            "h=1 outline left triangle should have at least 1 pixel"
        );
    }
}
