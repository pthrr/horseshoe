/// Text selection, search state, and word-boundary detection.
/// Text selection and click state.
#[derive(Default)]
pub struct SelectionState {
    pub start: Option<(u16, u16)>,
    pub end: Option<(u16, u16)>,
    pub active: bool,
    pub last_click_time: Option<std::time::Instant>,
    pub last_click_pos: (u16, u16),
    pub click_count: u32,
}

impl SelectionState {
    /// Return the normalized (start <= end) selection range, if both endpoints
    /// are set.
    pub const fn normalized_range(&self) -> Option<((u16, u16), (u16, u16))> {
        let (Some(s), Some(e)) = (self.start, self.end) else {
            return None;
        };
        Some(normalize_selection_range(s, e))
    }

    /// Clear the current selection, returning whether any clearing occurred.
    pub const fn clear(&mut self) -> bool {
        if self.start.is_some() || self.end.is_some() {
            self.start = None;
            self.end = None;
            self.active = false;
            true
        } else {
            false
        }
    }

    /// Select an entire line.
    pub const fn select_line(&mut self, row: u16, term_cols: u16) {
        self.start = Some((0, row));
        self.end = Some((term_cols.saturating_sub(1), row));
    }

    /// Register a click at `(col, row)` and update multi-click detection.
    ///
    /// Returns the click count after this click (1 = single, 2 = double,
    /// 3+ = triple).
    pub fn register_click(&mut self, col: u16, row: u16) -> u32 {
        let now = std::time::Instant::now();
        let is_multi = if let Some(last_time) = self.last_click_time {
            let elapsed = now.duration_since(last_time);
            let (lc, lr) = self.last_click_pos;
            elapsed.as_millis() < 300 && col == lc && row == lr
        } else {
            false
        };

        if is_multi {
            self.click_count += 1;
        } else {
            self.click_count = 1;
        }
        self.last_click_time = Some(now);
        self.last_click_pos = (col, row);
        self.click_count
    }
}

/// A search match position in the terminal grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    pub row: u16,
    pub start_col: u16,
    pub end_col: u16,
}

/// Scrollback search state.
#[derive(Default)]
pub struct SearchState {
    pub active: bool,
    pub query: String,
    /// Byte offset of the cursor within `query` (always on a char boundary).
    pub cursor_pos: usize,
    /// Cached lowercased query (updated when `query` changes).
    pub query_lower: String,
    pub matches: Vec<SearchMatch>,
    pub current_match: usize,
    /// Reusable buffer for row text extraction (avoids per-call allocation).
    pub row_texts: Vec<String>,
}

impl SearchState {
    /// Navigate to the next or previous match. Returns `true` if the current
    /// match index changed.
    pub const fn navigate(&mut self, forward: bool) -> bool {
        if self.matches.is_empty() {
            return false;
        }
        let old = self.current_match;
        if forward {
            self.current_match = (self.current_match + 1) % self.matches.len();
        } else if self.current_match == 0 {
            self.current_match = self.matches.len() - 1;
        } else {
            self.current_match -= 1;
        }
        self.current_match != old
    }
}

/// Normalize a selection range so that `start` is before `end` in
/// reading order (row-major, then column).
pub const fn normalize_selection_range(
    start: (u16, u16),
    end: (u16, u16),
) -> ((u16, u16), (u16, u16)) {
    if start.1 < end.1 || (start.1 == end.1 && start.0 <= end.0) {
        (start, end)
    } else {
        (end, start)
    }
}

/// Check whether a cell at `(row, col)` falls within the normalized selection
/// from `start` to `end`.
///
/// Both `start` and `end` must already be in normalized order (i.e.
/// `start` is before `end` in reading order).
pub const fn cell_in_selection(row: u16, col: u16, start: (u16, u16), end: (u16, u16)) -> bool {
    let (sc, sr) = start;
    let (ec, er) = end;

    if sr == er {
        // Single-row selection
        row == sr && col >= sc && col <= ec
    } else if row == sr {
        // First row of multi-row selection
        col >= sc
    } else if row == er {
        // Last row
        col <= ec
    } else {
        // Middle rows: entire row is selected
        row > sr && row < er
    }
}

/// Character class for word-boundary detection.
///
/// - 0 = word character (alphanumeric, `_`, `-`, `.`, `/`)
/// - 1 = whitespace
/// - 2 = punctuation / other
pub const fn char_class(ch: char) -> u8 {
    if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' || ch == '/' {
        0
    } else if ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' {
        1
    } else {
        // Non-ASCII alphanumeric handled below at runtime
        2
    }
}

/// Extended char-class that also handles Unicode alphanumerics.
///
/// This is the non-const version used in production; it calls
/// `char::is_alphanumeric()` which is not const-stable.
pub fn char_class_unicode(ch: char) -> u8 {
    if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.' || ch == '/' {
        0
    } else if ch.is_whitespace() {
        1
    } else {
        2
    }
}

/// Find word boundaries around `col` in a sorted list of `(column, char)` pairs.
///
/// Returns `(start_col, end_col)` for the word that contains or is nearest to
/// `col`.
pub fn word_boundaries(row_chars: &[(u16, char)], col: u16) -> (u16, u16) {
    if row_chars.is_empty() {
        return (col, col);
    }

    // Find the entry at (or nearest to) the click column
    let idx = row_chars
        .iter()
        .position(|&(c, _)| c >= col)
        .unwrap_or(row_chars.len().saturating_sub(1));
    let (_, click_ch) = row_chars.get(idx).copied().unwrap_or((col, ' '));

    let target_class = char_class_unicode(click_ch);

    // Scan left
    let mut start_idx = idx;
    while start_idx > 0 {
        let prev = start_idx - 1;
        if let Some(&(_, prev_ch)) = row_chars.get(prev) {
            if char_class_unicode(prev_ch) == target_class {
                start_idx = prev;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // Scan right
    let mut end_idx = idx;
    while end_idx + 1 < row_chars.len() {
        let next = end_idx + 1;
        if let Some(&(_, next_ch)) = row_chars.get(next) {
            if char_class_unicode(next_ch) == target_class {
                end_idx = next;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    let start_col = row_chars.get(start_idx).map_or(col, |&(c, _)| c);
    let end_col = row_chars.get(end_idx).map_or(col, |&(c, _)| c);
    (start_col, end_col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_forward_same_row() {
        let r = normalize_selection_range((2, 5), (8, 5));
        assert_eq!(r, ((2, 5), (8, 5)));
    }

    #[test]
    fn normalize_reverse_same_row() {
        let r = normalize_selection_range((8, 5), (2, 5));
        assert_eq!(r, ((2, 5), (8, 5)));
    }

    #[test]
    fn normalize_forward_multi_row() {
        let r = normalize_selection_range((5, 1), (3, 4));
        assert_eq!(r, ((5, 1), (3, 4)));
    }

    #[test]
    fn normalize_reverse_multi_row() {
        let r = normalize_selection_range((3, 4), (5, 1));
        assert_eq!(r, ((5, 1), (3, 4)));
    }

    #[test]
    fn normalize_same_cell() {
        let r = normalize_selection_range((3, 3), (3, 3));
        assert_eq!(r, ((3, 3), (3, 3)));
    }

    #[test]
    fn normalize_origin() {
        let r = normalize_selection_range((0, 0), (0, 0));
        assert_eq!(r, ((0, 0), (0, 0)));
    }

    #[test]
    fn normalize_extreme_values() {
        let r = normalize_selection_range((u16::MAX, u16::MAX), (0, 0));
        assert_eq!(r, ((0, 0), (u16::MAX, u16::MAX)));
    }

    #[test]
    fn normalize_adjacent_rows() {
        let r = normalize_selection_range((10, 3), (0, 4));
        assert_eq!(r, ((10, 3), (0, 4)));
    }

    #[test]
    fn cell_in_single_row_inside() {
        assert!(cell_in_selection(5, 3, (2, 5), (8, 5)));
    }

    #[test]
    fn cell_in_single_row_at_start() {
        assert!(cell_in_selection(5, 2, (2, 5), (8, 5)));
    }

    #[test]
    fn cell_in_single_row_at_end() {
        assert!(cell_in_selection(5, 8, (2, 5), (8, 5)));
    }

    #[test]
    fn cell_outside_single_row_before() {
        assert!(!cell_in_selection(5, 1, (2, 5), (8, 5)));
    }

    #[test]
    fn cell_outside_single_row_after() {
        assert!(!cell_in_selection(5, 9, (2, 5), (8, 5)));
    }

    #[test]
    fn cell_outside_different_row() {
        assert!(!cell_in_selection(4, 5, (2, 5), (8, 5)));
    }

    #[test]
    fn cell_in_first_row_of_multi() {
        assert!(cell_in_selection(1, 5, (3, 1), (7, 4)));
        assert!(cell_in_selection(1, 3, (3, 1), (7, 4)));
        assert!(cell_in_selection(1, 79, (3, 1), (7, 4)));
    }

    #[test]
    fn cell_in_first_row_before_start() {
        assert!(!cell_in_selection(1, 2, (3, 1), (7, 4)));
    }

    #[test]
    fn cell_in_last_row_of_multi() {
        assert!(cell_in_selection(4, 0, (3, 1), (7, 4)));
        assert!(cell_in_selection(4, 7, (3, 1), (7, 4)));
    }

    #[test]
    fn cell_in_last_row_after_end() {
        assert!(!cell_in_selection(4, 8, (3, 1), (7, 4)));
    }

    #[test]
    fn cell_in_middle_row() {
        assert!(cell_in_selection(2, 0, (3, 1), (7, 4)));
        assert!(cell_in_selection(3, 50, (3, 1), (7, 4)));
    }

    #[test]
    fn cell_outside_above() {
        assert!(!cell_in_selection(0, 5, (3, 1), (7, 4)));
    }

    #[test]
    fn class_alpha() {
        assert_eq!(char_class('a'), 0);
        assert_eq!(char_class('Z'), 0);
        assert_eq!(char_class('5'), 0);
    }

    #[test]
    fn class_word_chars() {
        assert_eq!(char_class('_'), 0);
        assert_eq!(char_class('-'), 0);
        assert_eq!(char_class('.'), 0);
        assert_eq!(char_class('/'), 0);
    }

    #[test]
    fn class_whitespace() {
        assert_eq!(char_class(' '), 1);
        assert_eq!(char_class('\t'), 1);
        assert_eq!(char_class('\n'), 1);
    }

    #[test]
    fn class_punctuation() {
        assert_eq!(char_class('@'), 2);
        assert_eq!(char_class('#'), 2);
        assert_eq!(char_class('!'), 2);
        assert_eq!(char_class('('), 2);
    }

    #[test]
    fn class_unicode_alpha() {
        // char_class_unicode handles non-ASCII alphanumeric
        assert_eq!(char_class_unicode('ä'), 0);
        assert_eq!(char_class_unicode('日'), 0);
    }

    #[test]
    fn word_boundaries_middle_of_word() {
        let row: Vec<(u16, char)> = "hello world"
            .chars()
            .enumerate()
            .map(|(i, c)| (u16::try_from(i).expect("column index fits u16"), c))
            .collect();
        let (s, e) = word_boundaries(&row, 2);
        assert_eq!(s, 0);
        assert_eq!(e, 4);
    }

    #[test]
    fn word_boundaries_start_of_word() {
        let row: Vec<(u16, char)> = "hello world"
            .chars()
            .enumerate()
            .map(|(i, c)| (u16::try_from(i).expect("column index fits u16"), c))
            .collect();
        let (s, e) = word_boundaries(&row, 0);
        assert_eq!(s, 0);
        assert_eq!(e, 4);
    }

    #[test]
    fn word_boundaries_end_of_word() {
        let row: Vec<(u16, char)> = "hello world"
            .chars()
            .enumerate()
            .map(|(i, c)| (u16::try_from(i).expect("column index fits u16"), c))
            .collect();
        let (s, e) = word_boundaries(&row, 4);
        assert_eq!(s, 0);
        assert_eq!(e, 4);
    }

    #[test]
    fn word_boundaries_on_space() {
        let row: Vec<(u16, char)> = "hello world"
            .chars()
            .enumerate()
            .map(|(i, c)| (u16::try_from(i).expect("column index fits u16"), c))
            .collect();
        let (s, e) = word_boundaries(&row, 5);
        assert_eq!(s, 5);
        assert_eq!(e, 5);
    }

    #[test]
    fn word_boundaries_punctuation() {
        let row: Vec<(u16, char)> = "foo::bar"
            .chars()
            .enumerate()
            .map(|(i, c)| (u16::try_from(i).expect("column index fits u16"), c))
            .collect();
        // Click on first colon
        let (s, e) = word_boundaries(&row, 3);
        assert_eq!(s, 3);
        assert_eq!(e, 4);
    }

    #[test]
    fn word_boundaries_path() {
        let row: Vec<(u16, char)> = "/usr/bin/ls"
            .chars()
            .enumerate()
            .map(|(i, c)| (u16::try_from(i).expect("column index fits u16"), c))
            .collect();
        let (s, e) = word_boundaries(&row, 5);
        assert_eq!(s, 0);
        assert_eq!(e, 10);
    }

    #[test]
    fn word_boundaries_empty() {
        let (s, e) = word_boundaries(&[], 5);
        assert_eq!(s, 5);
        assert_eq!(e, 5);
    }

    #[test]
    fn word_boundaries_single_char() {
        let row = vec![(0, 'x')];
        let (s, e) = word_boundaries(&row, 0);
        assert_eq!(s, 0);
        assert_eq!(e, 0);
    }

    #[test]
    fn selection_state_default() {
        let s = SelectionState::default();
        assert!(s.start.is_none());
        assert!(s.end.is_none());
        assert!(!s.active);
        assert!(s.last_click_time.is_none());
        assert_eq!(s.last_click_pos, (0, 0));
        assert_eq!(s.click_count, 0);
    }

    #[test]
    fn selection_state_clear_empty() {
        let mut s = SelectionState::default();
        assert!(!s.clear());
    }

    #[test]
    fn selection_state_clear_active() {
        let mut s = SelectionState {
            start: Some((0, 0)),
            end: Some((5, 5)),
            active: true,
            ..SelectionState::default()
        };
        assert!(s.clear());
        assert!(s.start.is_none());
        assert!(s.end.is_none());
        assert!(!s.active);
    }

    #[test]
    fn selection_state_select_line() {
        let mut s = SelectionState::default();
        s.select_line(3, 80);
        assert_eq!(s.start, Some((0, 3)));
        assert_eq!(s.end, Some((79, 3)));
    }

    #[test]
    fn selection_state_select_line_min_cols() {
        let mut s = SelectionState::default();
        s.select_line(0, 1);
        assert_eq!(s.start, Some((0, 0)));
        assert_eq!(s.end, Some((0, 0)));
    }

    #[test]
    fn selection_state_normalized_range_none() {
        let s = SelectionState::default();
        assert!(s.normalized_range().is_none());
    }

    #[test]
    fn selection_state_normalized_range_forward() {
        let s = SelectionState {
            start: Some((2, 1)),
            end: Some((5, 3)),
            ..SelectionState::default()
        };
        let r = s.normalized_range().expect("should have range");
        assert_eq!(r, ((2, 1), (5, 3)));
    }

    #[test]
    fn selection_state_normalized_range_reverse() {
        let s = SelectionState {
            start: Some((5, 3)),
            end: Some((2, 1)),
            ..SelectionState::default()
        };
        let r = s.normalized_range().expect("should have range");
        assert_eq!(r, ((2, 1), (5, 3)));
    }

    #[test]
    fn selection_state_register_click_single() {
        let mut s = SelectionState::default();
        let count = s.register_click(5, 3);
        assert_eq!(count, 1);
        assert_eq!(s.last_click_pos, (5, 3));
        assert!(s.last_click_time.is_some());
    }

    #[test]
    fn selection_state_register_click_double() {
        let mut s = SelectionState::default();
        let _ = s.register_click(5, 3);
        // Second click at same position immediately
        let count = s.register_click(5, 3);
        assert_eq!(count, 2);
    }

    #[test]
    fn selection_state_register_click_different_pos() {
        let mut s = SelectionState::default();
        let _ = s.register_click(5, 3);
        let count = s.register_click(10, 3);
        assert_eq!(count, 1);
    }

    #[test]
    fn search_state_default() {
        let s = SearchState::default();
        assert!(!s.active);
        assert!(s.query.is_empty());
        assert!(s.matches.is_empty());
        assert_eq!(s.current_match, 0);
    }

    #[test]
    fn search_navigate_forward() {
        let mut s = SearchState {
            matches: vec![
                SearchMatch {
                    row: 0,
                    start_col: 0,
                    end_col: 3,
                },
                SearchMatch {
                    row: 1,
                    start_col: 5,
                    end_col: 8,
                },
                SearchMatch {
                    row: 2,
                    start_col: 0,
                    end_col: 2,
                },
            ],
            ..SearchState::default()
        };
        assert!(s.navigate(true));
        assert_eq!(s.current_match, 1);
        assert!(s.navigate(true));
        assert_eq!(s.current_match, 2);
        // Wrap around
        assert!(s.navigate(true));
        assert_eq!(s.current_match, 0);
    }

    #[test]
    fn search_navigate_backward() {
        let mut s = SearchState {
            matches: vec![
                SearchMatch {
                    row: 0,
                    start_col: 0,
                    end_col: 3,
                },
                SearchMatch {
                    row: 1,
                    start_col: 5,
                    end_col: 8,
                },
            ],
            ..SearchState::default()
        };
        // Backward from 0 wraps to last
        assert!(s.navigate(false));
        assert_eq!(s.current_match, 1);
        assert!(s.navigate(false));
        assert_eq!(s.current_match, 0);
    }

    #[test]
    fn search_navigate_empty() {
        let mut s = SearchState::default();
        assert!(!s.navigate(true));
        assert!(!s.navigate(false));
    }

    #[test]
    fn search_navigate_single_match() {
        let mut s = SearchState {
            matches: vec![SearchMatch {
                row: 0,
                start_col: 0,
                end_col: 3,
            }],
            ..SearchState::default()
        };
        // Single match: forward wraps to itself -- no change
        assert!(!s.navigate(true));
        assert_eq!(s.current_match, 0);
    }

    #[test]
    fn search_match_equality() {
        let a = SearchMatch {
            row: 1,
            start_col: 2,
            end_col: 5,
        };
        let b = SearchMatch {
            row: 1,
            start_col: 2,
            end_col: 5,
        };
        let c = SearchMatch {
            row: 1,
            start_col: 2,
            end_col: 6,
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn word_boundaries_at_col_zero() {
        let row: Vec<(u16, char)> = "hello"
            .chars()
            .enumerate()
            .map(|(i, c)| (u16::try_from(i).expect("fits"), c))
            .collect();
        let (s, e) = word_boundaries(&row, 0);
        assert_eq!(s, 0, "start should be 0 at beginning of word");
        assert_eq!(e, 4, "end should be last char of word");
    }

    #[test]
    fn word_boundaries_at_last_col() {
        let row: Vec<(u16, char)> = "hello"
            .chars()
            .enumerate()
            .map(|(i, c)| (u16::try_from(i).expect("fits"), c))
            .collect();
        let (s, e) = word_boundaries(&row, 4);
        assert_eq!(s, 0);
        assert_eq!(e, 4, "should select whole word from last col");
    }

    #[test]
    fn cell_in_selection_u16_max() {
        // Selection spanning extreme coordinate values
        let max = u16::MAX;
        assert!(cell_in_selection(max, max, (0, max), (max, max)));
        assert!(cell_in_selection(max, 0, (0, max), (max, max)));
        assert!(!cell_in_selection(max - 1, 0, (1, max), (max, max)));
    }

    #[test]
    fn test_normalize_same_cell_returns_identity() {
        let r = normalize_selection_range((5, 5), (5, 5));
        assert_eq!(r.0, r.1, "same cell should produce start==end");
    }

    #[test]
    fn test_search_match_last_col() {
        // Ensure SearchMatch can represent the last column without off-by-one
        let m = SearchMatch {
            row: 0,
            start_col: 79,
            end_col: 79,
        };
        assert_eq!(m.start_col, m.end_col, "single-char match at last col");
    }

    #[test]
    fn test_selection_empty_grid() {
        let row_chars: Vec<(u16, char)> = Vec::new();
        let (s, e) = word_boundaries(&row_chars, 5);
        assert_eq!(
            (s, e),
            (5, 5),
            "empty grid should return click col for both bounds"
        );
    }

    #[test]
    fn test_register_click_triple_click() {
        let mut s = SelectionState::default();
        let c1 = s.register_click(5, 3);
        assert_eq!(c1, 1);
        let c2 = s.register_click(5, 3);
        assert_eq!(c2, 2);
        let c3 = s.register_click(5, 3);
        assert_eq!(c3, 3);
    }

    #[test]
    fn test_register_click_timeout_resets() {
        let mut s = SelectionState::default();
        let _ = s.register_click(5, 3);
        assert_eq!(s.click_count, 1);
        // Simulate 300ms elapsed by backdating last_click_time
        s.last_click_time =
            std::time::Instant::now().checked_sub(std::time::Duration::from_millis(400));
        let count = s.register_click(5, 3);
        assert_eq!(count, 1, "click count should reset after timeout");
    }

    #[test]
    fn test_word_boundaries_click_past_end() {
        // Row has 5 chars at columns 0-4
        let row_chars: Vec<(u16, char)> = "hello"
            .chars()
            .enumerate()
            .map(|(i, c)| (u16::try_from(i).expect("fits"), c))
            .collect();
        // Click at column 100 (far past end) should not panic
        let (s, e) = word_boundaries(&row_chars, 100);
        // Falls back to last entry index; entire word "hello" is class 0
        assert_eq!(s, 0);
        assert_eq!(e, 4);
    }

    #[test]
    fn test_cell_in_selection_single_row() {
        // Selection on row 5 from col 3 to col 7
        let start = (3, 5);
        let end = (7, 5);
        // Before selection
        assert!(!cell_in_selection(5, 2, start, end));
        // At start boundary
        assert!(cell_in_selection(5, 3, start, end));
        // Inside
        assert!(cell_in_selection(5, 5, start, end));
        // At end boundary
        assert!(cell_in_selection(5, 7, start, end));
        // After selection
        assert!(!cell_in_selection(5, 8, start, end));
        // Wrong row
        assert!(!cell_in_selection(4, 5, start, end));
        assert!(!cell_in_selection(6, 5, start, end));
    }

    #[test]
    fn test_cell_in_selection_full_grid() {
        // Multi-row selection: row 2 col 5 to row 6 col 10
        let start = (5, 2);
        let end = (10, 6);
        // Middle rows (3, 4, 5) should be fully selected at any column
        for row in 3..=5 {
            assert!(cell_in_selection(row, 0, start, end), "row {row} col 0");
            assert!(cell_in_selection(row, 40, start, end), "row {row} col 40");
            assert!(
                cell_in_selection(row, u16::MAX, start, end),
                "row {row} col MAX"
            );
        }
        // First row: only from col 5 onward
        assert!(!cell_in_selection(2, 4, start, end));
        assert!(cell_in_selection(2, 5, start, end));
        assert!(cell_in_selection(2, 79, start, end));
        // Last row: only up to col 10
        assert!(cell_in_selection(6, 0, start, end));
        assert!(cell_in_selection(6, 10, start, end));
        assert!(!cell_in_selection(6, 11, start, end));
    }

    #[test]
    fn test_normalize_selection_range_same_point() {
        let r = normalize_selection_range((7, 3), (7, 3));
        assert_eq!(r, ((7, 3), (7, 3)));
        // start == end means the range is a single cell
        assert_eq!(r.0, r.1);
    }

    #[test]
    fn test_search_navigate_single_match() {
        let mut s = SearchState {
            matches: vec![SearchMatch {
                row: 5,
                start_col: 2,
                end_col: 8,
            }],
            current_match: 0,
            ..SearchState::default()
        };
        // Forward on single match wraps to 0 -- no change
        assert!(!s.navigate(true));
        assert_eq!(s.current_match, 0);
        // Backward on single match also wraps to 0 -- no change
        assert!(!s.navigate(false));
        assert_eq!(s.current_match, 0);
    }

    #[test]
    fn test_search_navigate_empty_matches() {
        let mut s = SearchState::default();
        assert!(!s.navigate(true), "forward on empty should return false");
        assert!(!s.navigate(false), "backward on empty should return false");
        assert_eq!(s.current_match, 0);
    }

    #[test]
    fn test_selection_clear_already_empty() {
        let mut s = SelectionState::default();
        assert!(
            !s.clear(),
            "clearing default (empty) state should return false"
        );
        // Verify still in default state
        assert!(s.start.is_none());
        assert!(s.end.is_none());
        assert!(!s.active);
    }

    #[test]
    fn test_char_class_unicode_emoji() {
        // Emoji are not alphanumeric and not whitespace, so class 2 (punctuation)
        assert_eq!(char_class_unicode('\u{1F600}'), 2); // grinning face
        assert_eq!(char_class_unicode('\u{2764}'), 2); // heavy heart
        assert_eq!(char_class_unicode('\u{1F4A9}'), 2); // pile of poo
    }
}
