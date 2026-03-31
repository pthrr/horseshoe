/// Bracketed paste safety checks and PTY write preparation.
/// The bracketed-paste end delimiter. Text containing this sequence can
/// break out of bracketed-paste mode and must always be rejected.
const BRACKET_END: &[u8] = b"\x1b[201~";

/// Check whether `text` is safe to paste into a terminal (strict).
///
/// Wraps `libghostty_vt::paste::is_safe`. Rejects text with
/// newlines OR the bracket-end sequence. Use [`contains_bracket_end`]
/// for a bracketed-mode-aware check that allows newlines.
pub fn is_paste_safe(text: &str) -> bool {
    libghostty_vt::paste::is_safe(text)
}

/// Return true if `text` contains the bracket-paste end sequence
/// (`ESC[201~`), which would allow an attacker to escape bracketed
/// paste mode.
pub fn contains_bracket_end(text: &str) -> bool {
    text.as_bytes()
        .windows(BRACKET_END.len())
        .any(|w| w == BRACKET_END)
}

/// Prepare paste bytes for writing to the PTY.
///
/// When `bracketed` is true the text is wrapped in paste delimiters and
/// newlines are kept as-is (the application handles them inside brackets).
///
/// When `bracketed` is false, bare `\n` is converted to `\r` (the
/// terminal line discipline expects CR, not LF) and no delimiters are
/// added.
///
/// Returns `None` if the text contains the bracket-end escape sequence.
pub fn prepare_paste(text: &str, bracketed: bool) -> Option<Vec<u8>> {
    if contains_bracket_end(text) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() + if bracketed { 12 } else { 0 });
    if bracketed {
        out.extend_from_slice(b"\x1b[200~");
        out.extend_from_slice(text.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
    } else {
        // Convert \n → \r for non-bracketed mode (terminal expects CR).
        for &b in text.as_bytes() {
            out.push(if b == b'\n' { b'\r' } else { b });
        }
    }
    Some(out)
}

/// Assemble the bracketed-paste byte sequences for `text`.
///
/// When `bracketed` is true, returns the opening delimiter, the text bytes,
/// and the closing delimiter.  When false, returns only the text bytes
/// (the other two slices are empty).
///
/// Note: this does NOT convert `\n` → `\r` for non-bracketed mode.
/// Prefer [`prepare_paste`] for the full paste pipeline.
pub const fn bracket_paste(text: &str, bracketed: bool) -> [&[u8]; 3] {
    if bracketed {
        [b"\x1b[200~", text.as_bytes(), b"\x1b[201~"]
    } else {
        [b"", text.as_bytes(), b""]
    }
}

/// Total byte length of a bracketed paste sequence.
pub const fn bracket_paste_len(text: &str, bracketed: bool) -> usize {
    let parts = bracket_paste(text, bracketed);
    parts[0].len() + parts[1].len() + parts[2].len()
}

/// Read from a pipe fd with a configurable deadline.
///
/// Sets `O_NONBLOCK` on the fd, then polls in a loop with short sleeps.
/// Returns `None` if the deadline expires or an error occurs.
pub fn read_pipe_with_deadline(
    pipe: &mut (impl std::io::Read + std::os::fd::AsFd),
    deadline: std::time::Instant,
) -> Option<String> {
    use nix::fcntl::{FcntlArg, OFlag, fcntl};

    if let Ok(flags) = fcntl(pipe.as_fd(), FcntlArg::F_GETFL) {
        let oflags = OFlag::from_bits_truncate(flags);
        let _ = fcntl(pipe.as_fd(), FcntlArg::F_SETFL(oflags | OFlag::O_NONBLOCK));
    }

    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        match std::io::Read::read(pipe, &mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                if let Some(slice) = tmp.get(..n) {
                    buf.extend_from_slice(slice);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if !buf.is_empty() {
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(_) => break,
        }
    }
    String::from_utf8(buf).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn paste_safe_plain_text() {
        assert!(is_paste_safe("hello world"));
    }

    #[test]
    fn paste_safe_empty() {
        assert!(is_paste_safe(""));
    }

    #[test]
    fn paste_unsafe_newline() {
        // ghostty considers bare newlines unsafe (potential command injection)
        assert!(!is_paste_safe("line1\nline2"));
    }

    #[test]
    fn paste_unsafe_crlf() {
        // ghostty considers CRLF unsafe
        assert!(!is_paste_safe("line1\r\nline2"));
    }

    #[test]
    fn paste_safe_unicode() {
        assert!(is_paste_safe("日本語テスト 🎉"));
    }

    #[test]
    fn paste_safe_tab() {
        assert!(is_paste_safe("col1\tcol2"));
    }

    #[test]
    fn paste_unsafe_bracket_end() {
        // The bracket paste end sequence should be unsafe
        assert!(!is_paste_safe("\x1b[201~"));
    }

    #[test]
    fn paste_safe_no_escape() {
        // Ensure plain text with no ESC is safe
        assert!(is_paste_safe(
            "just regular text with spaces and numbers 12345"
        ));
    }

    #[test]
    fn bracket_paste_bracketed() {
        let parts = bracket_paste("hello", true);
        assert_eq!(parts[0], b"\x1b[200~");
        assert_eq!(parts[1], b"hello");
        assert_eq!(parts[2], b"\x1b[201~");
    }

    #[test]
    fn bracket_paste_unbracketed() {
        let parts = bracket_paste("hello", false);
        assert_eq!(parts[0], b"");
        assert_eq!(parts[1], b"hello");
        assert_eq!(parts[2], b"");
    }

    #[test]
    fn bracket_paste_empty_text() {
        let parts = bracket_paste("", true);
        assert_eq!(parts[0], b"\x1b[200~");
        assert_eq!(parts[1], b"");
        assert_eq!(parts[2], b"\x1b[201~");
    }

    #[test]
    fn bracket_paste_special_chars() {
        let text = "\x1b[A\n\r\t";
        let parts = bracket_paste(text, true);
        assert_eq!(parts[1], text.as_bytes());
    }

    #[test]
    fn bracket_paste_len_bracketed() {
        assert_eq!(bracket_paste_len("hello", true), 6 + 5 + 6); // \x1b[200~ + hello + \x1b[201~
    }

    #[test]
    fn bracket_paste_len_unbracketed() {
        assert_eq!(bracket_paste_len("hello", false), 5);
    }

    #[test]
    fn bracket_paste_large_text() {
        let text = "a".repeat(10_000);
        let parts = bracket_paste(&text, true);
        assert_eq!(parts[1].len(), 10_000);
    }

    #[test]
    fn bracket_end_absent() {
        assert!(!contains_bracket_end("hello world"));
    }

    #[test]
    fn bracket_end_present() {
        assert!(contains_bracket_end("foo\x1b[201~bar"));
    }

    #[test]
    fn bracket_end_empty() {
        assert!(!contains_bracket_end(""));
    }

    #[test]
    fn bracket_end_partial_sequence() {
        assert!(!contains_bracket_end("\x1b[201"));
    }

    #[test]
    fn bracket_end_with_newlines() {
        // Newlines are fine — only bracket-end is dangerous
        assert!(!contains_bracket_end("line1\nline2\nline3"));
    }

    #[test]
    fn prepare_paste_bracketed_plain() {
        let data = prepare_paste("hello", true).expect("should succeed");
        assert_eq!(data, b"\x1b[200~hello\x1b[201~");
    }

    #[test]
    fn prepare_paste_bracketed_with_newlines() {
        let data = prepare_paste("line1\nline2", true).expect("should succeed");
        // Newlines preserved in bracketed mode
        assert_eq!(data, b"\x1b[200~line1\nline2\x1b[201~");
    }

    #[test]
    fn prepare_paste_unbracketed_plain() {
        let data = prepare_paste("hello", false).expect("should succeed");
        assert_eq!(data, b"hello");
    }

    #[test]
    fn prepare_paste_unbracketed_newline_to_cr() {
        let data = prepare_paste("line1\nline2", false).expect("should succeed");
        // \n converted to \r in non-bracketed mode
        assert_eq!(data, b"line1\rline2");
    }

    #[test]
    fn prepare_paste_unbracketed_crlf() {
        let data = prepare_paste("a\r\nb", false).expect("should succeed");
        // \r stays, \n → \r → results in \r\r
        assert_eq!(data, b"a\r\rb");
    }

    #[test]
    fn prepare_paste_rejects_bracket_end_bracketed() {
        assert!(prepare_paste("foo\x1b[201~bar", true).is_none());
    }

    #[test]
    fn prepare_paste_rejects_bracket_end_unbracketed() {
        assert!(prepare_paste("foo\x1b[201~bar", false).is_none());
    }

    #[test]
    fn prepare_paste_empty_bracketed() {
        let data = prepare_paste("", true).expect("should succeed");
        assert_eq!(data, b"\x1b[200~\x1b[201~");
    }

    #[test]
    fn prepare_paste_empty_unbracketed() {
        let data = prepare_paste("", false).expect("should succeed");
        assert!(data.is_empty());
    }

    #[test]
    fn prepare_paste_unicode() {
        let data = prepare_paste("日本語\n🎉", true).expect("should succeed");
        assert!(data.starts_with(b"\x1b[200~"));
        assert!(data.ends_with(b"\x1b[201~"));
    }

    /// Create a Unix pipe and return `(read_file, write_file)` as `std::fs::File`.
    fn make_pipe() -> (std::fs::File, std::fs::File) {
        let (rx, tx) = nix::unistd::pipe().expect("pipe");
        (std::fs::File::from(rx), std::fs::File::from(tx))
    }

    #[test]
    fn pipe_read_immediate_data() {
        let (mut rx, mut tx) = make_pipe();
        std::io::Write::write_all(&mut tx, b"hello").expect("write");
        drop(tx);

        let deadline = Instant::now() + Duration::from_secs(5);
        let result = read_pipe_with_deadline(&mut rx, deadline);
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn pipe_read_closed_empty() {
        let (mut rx, tx) = make_pipe();
        drop(tx);

        let deadline = Instant::now() + Duration::from_secs(5);
        let result = read_pipe_with_deadline(&mut rx, deadline);
        assert_eq!(result, Some(String::new()));
    }

    #[test]
    fn pipe_read_timeout_short_deadline() {
        let (mut rx, _tx) = make_pipe();

        let deadline = Instant::now() + Duration::from_millis(50);
        let result = read_pipe_with_deadline(&mut rx, deadline);
        assert!(result.is_none(), "should timeout with no data");
    }

    #[test]
    fn pipe_read_large_data() {
        let (mut rx, mut tx) = make_pipe();
        let data = "x".repeat(8192);
        std::io::Write::write_all(&mut tx, data.as_bytes()).expect("write");
        drop(tx);

        let deadline = Instant::now() + Duration::from_secs(5);
        let result = read_pipe_with_deadline(&mut rx, deadline);
        assert!(result.is_some());
        let text = result.expect("should have data");
        assert_eq!(text.len(), 8192);
    }

    #[test]
    fn pipe_read_invalid_utf8() {
        let (mut rx, mut tx) = make_pipe();
        std::io::Write::write_all(&mut tx, &[0xFF, 0xFE, 0x80]).expect("write");
        drop(tx);

        let deadline = Instant::now() + Duration::from_secs(5);
        let result = read_pipe_with_deadline(&mut rx, deadline);
        assert!(result.is_none(), "invalid UTF-8 should return None");
    }

    #[test]
    fn prepare_paste_with_null_bytes() {
        let text = "hello\x00world";
        let data = prepare_paste(text, true);
        // Should succeed — null bytes are not the bracket-end sequence
        assert!(data.is_some(), "null bytes should not block paste");
        let bytes = data.expect("paste data");
        assert!(
            bytes.windows(1).any(|w| w == [0]),
            "null byte should be preserved"
        );
    }

    #[test]
    fn prepare_paste_large_text() {
        let text = "a".repeat(1_000_000);
        let data = prepare_paste(&text, true).expect("1MB paste should succeed");
        // 6 (open) + 1_000_000 + 6 (close)
        assert_eq!(data.len(), 1_000_012);
    }

    #[test]
    fn is_paste_safe_all_control_chars() {
        for byte in 0x00u8..=0x1F {
            let text = String::from(char::from(byte));
            let safe = is_paste_safe(&text);
            // Tab (0x09) is safe; newline/CR/other control chars are unsafe per ghostty
            if byte == 0x09 {
                assert!(safe, "tab (0x09) should be safe");
            }
            // Just ensure no panics — the safety classification is upstream's decision
        }
    }

    #[test]
    fn test_bracket_paste_contains_delimiter() {
        // Text containing the bracket START sequence should still work
        let text = "before\x1b[200~after";
        let data = prepare_paste(text, true);
        assert!(data.is_some(), "bracket start in payload should be OK");
    }

    #[test]
    fn test_prepare_paste_empty_unbracketed_is_empty() {
        let data = prepare_paste("", false).expect("should succeed");
        assert!(
            data.is_empty(),
            "empty unbracketed paste should be empty vec"
        );
    }

    #[test]
    fn test_paste_bel_and_del() {
        // BEL (0x07) and DEL (0x7F) should not trigger rejection
        let bel_input = "hello\x07world";
        let delete_input = "hello\x7Fworld";
        assert!(
            !contains_bracket_end(bel_input),
            "BEL should not match bracket end"
        );
        assert!(
            !contains_bracket_end(delete_input),
            "DEL should not match bracket end"
        );
        // prepare_paste should work for these
        assert!(prepare_paste(bel_input, true).is_some());
        assert!(prepare_paste(delete_input, true).is_some());
    }

    #[test]
    fn test_is_paste_safe_tab_only() {
        assert!(is_paste_safe("\t"), "a single tab should be safe");
    }

    #[test]
    fn test_is_paste_safe_mixed_safe() {
        assert!(
            is_paste_safe("hello world\t"),
            "text with trailing tab should be safe"
        );
    }

    #[test]
    fn test_contains_bracket_end_partial() {
        // "\x1b[20" without trailing "1~" is NOT the bracket end sequence
        assert!(
            !contains_bracket_end("\x1b[20"),
            "partial bracket-end sequence should not match"
        );
        // Also test with just the tilde missing
        assert!(
            !contains_bracket_end("\x1b[201"),
            "bracket-end missing final tilde should not match"
        );
    }

    #[test]
    fn test_prepare_paste_empty_string() {
        // Bracketed: returns the delimiters wrapping empty content
        let bracketed = prepare_paste("", true).expect("should succeed");
        assert_eq!(bracketed, b"\x1b[200~\x1b[201~");
        // Unbracketed: returns empty vec
        let unbracketed = prepare_paste("", false).expect("should succeed");
        assert!(unbracketed.is_empty());
    }

    #[test]
    fn test_bracket_paste_len_consistency() {
        let texts = ["", "hello", "a\nb\nc", "\x1b[200~nested"];
        for text in texts {
            for bracketed in [true, false] {
                let parts = bracket_paste(text, bracketed);
                let sum: usize = parts.iter().map(|p| p.len()).sum();
                assert_eq!(
                    bracket_paste_len(text, bracketed),
                    sum,
                    "bracket_paste_len should equal sum of parts for {text:?} bracketed={bracketed}",
                );
            }
        }
    }

    #[test]
    fn test_prepare_paste_large_text() {
        let text = "x".repeat(10_000);
        let data = prepare_paste(&text, true).expect("10KB paste should succeed");
        // 6 (open) + 10_000 + 6 (close)
        assert_eq!(data.len(), 10_012);
        let data_unbr = prepare_paste(&text, false).expect("10KB unbracketed should succeed");
        assert_eq!(data_unbr.len(), 10_000);
    }
}
