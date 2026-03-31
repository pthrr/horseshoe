use std::path::PathBuf;

/// States for scanning OSC 52 clipboard sequences from raw PTY output.
/// OSC 52 format: ESC ] 52 ; <sel> ; <base64> ST   (ST = ESC \ or BEL)
#[derive(Debug, Default, Clone)]
pub enum Osc52State {
    #[default]
    Normal,
    /// Saw ESC (0x1b)
    Esc,
    /// Saw ESC ]
    OscStart,
    /// Saw ESC ] 5
    Osc5,
    /// Saw ESC ] 52
    Osc52,
    /// Saw ESC ] 52 ; — accumulating selection target char(s)
    Target,
    /// Saw ESC ] 52 ; <sel> ; — accumulating base64 data
    Data,
    /// Inside data, saw ESC (potential ST = ESC \)
    DataEsc,
}

/// Scan a byte buffer for OSC 52 clipboard-set sequences.
/// Returns decoded clipboard text if a complete OSC 52 was found.
/// The state is carried across calls to handle sequences split across reads.
pub fn scan_osc52(state: &mut Osc52State, buf: &mut Vec<u8>, data: &[u8]) -> Option<String> {
    let mut result = None;
    for &b in data {
        match state {
            Osc52State::Normal => {
                if b == 0x1b {
                    *state = Osc52State::Esc;
                }
            }
            Osc52State::Esc => {
                if b == b']' {
                    *state = Osc52State::OscStart;
                    buf.clear();
                } else {
                    *state = Osc52State::Normal;
                }
            }
            Osc52State::OscStart => {
                if b == b'5' {
                    *state = Osc52State::Osc5;
                } else {
                    // Not OSC 52 — skip this OSC sequence
                    *state = Osc52State::Normal;
                }
            }
            Osc52State::Osc5 => {
                if b == b'2' {
                    *state = Osc52State::Osc52;
                } else {
                    *state = Osc52State::Normal;
                }
            }
            Osc52State::Osc52 => {
                if b == b';' {
                    *state = Osc52State::Target;
                } else {
                    *state = Osc52State::Normal;
                }
            }
            Osc52State::Target => {
                if b == b';' {
                    *state = Osc52State::Data;
                    buf.clear();
                } else if b == 0x1b || b == 0x07 {
                    // Terminated before data — query, not a set
                    *state = Osc52State::Normal;
                }
                // else: accumulate target char (c/p/s/etc.), stay in Target
            }
            Osc52State::Data => {
                if b == 0x07 {
                    // BEL terminator — decode
                    result = decode_osc52_base64(buf);
                    buf.clear();
                    *state = Osc52State::Normal;
                } else if b == 0x1b {
                    *state = Osc52State::DataEsc;
                } else {
                    buf.push(b);
                }
            }
            Osc52State::DataEsc => {
                if b == b'\\' {
                    // ST terminator (ESC \) — decode
                    result = decode_osc52_base64(buf);
                    buf.clear();
                    *state = Osc52State::Normal;
                } else {
                    // Not a proper ST, treat ESC as data
                    buf.push(0x1b);
                    buf.push(b);
                    *state = Osc52State::Data;
                }
            }
        }
    }
    result
}

/// Decode base64 data from an OSC 52 payload into a UTF-8 string.
pub(super) fn decode_osc52_base64(b64: &[u8]) -> Option<String> {
    use base64::Engine as _;
    use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
    // Accept both padded and unpadded base64 (terminals may omit padding).
    let engine = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
    );
    let bytes = engine.decode(b64).ok()?;
    String::from_utf8(bytes).ok()
}

/// Lightweight OSC accumulator that captures full OSC payloads.
/// When a complete OSC is received, returns the payload as a string.
#[derive(Debug, Default, Clone)]
pub struct OscAccum {
    state: OscAccumState,
    buf: Vec<u8>,
}

#[derive(Debug, Default, Clone, Copy)]
enum OscAccumState {
    #[default]
    Normal,
    Esc,
    Body,
    BodyEsc,
}

/// Result of scanning for general OSC sequences.
pub enum OscEvent {
    /// OSC 7: current working directory.
    Cwd(PathBuf),
    /// OSC 133;A: prompt start at the given cursor row
    PromptMark,
}

impl OscAccum {
    /// Feed bytes through the accumulator. Returns completed OSC events.
    pub fn feed(&mut self, data: &[u8]) -> Vec<OscEvent> {
        let mut events = Vec::new();
        for &b in data {
            match self.state {
                OscAccumState::Normal => {
                    if b == 0x1b {
                        self.state = OscAccumState::Esc;
                    }
                }
                OscAccumState::Esc => {
                    if b == b']' {
                        self.state = OscAccumState::Body;
                        self.buf.clear();
                    } else {
                        self.state = OscAccumState::Normal;
                    }
                }
                OscAccumState::Body => {
                    if b == 0x07 {
                        // BEL terminator
                        self.process_osc(&mut events);
                        self.buf.clear();
                        self.state = OscAccumState::Normal;
                    } else if b == 0x1b {
                        self.state = OscAccumState::BodyEsc;
                    } else {
                        self.buf.push(b);
                    }
                }
                OscAccumState::BodyEsc => {
                    if b == b'\\' {
                        // ST terminator
                        self.process_osc(&mut events);
                        self.buf.clear();
                        self.state = OscAccumState::Normal;
                    } else {
                        self.buf.push(0x1b);
                        self.buf.push(b);
                        self.state = OscAccumState::Body;
                    }
                }
            }
        }
        events
    }

    fn process_osc(&self, events: &mut Vec<OscEvent>) {
        let payload = String::from_utf8_lossy(&self.buf);
        if let Some(url) = payload.strip_prefix("7;") {
            // OSC 7: file:///path/to/cwd
            if let Some(path_str) = url.strip_prefix("file://") {
                // Strip optional hostname: file://hostname/path → /path
                let path = if let Some(idx) = path_str.find('/') {
                    &path_str[idx..]
                } else {
                    path_str
                };
                events.push(OscEvent::Cwd(PathBuf::from(path)));
            }
        } else if payload.starts_with("133;A") {
            events.push(OscEvent::PromptMark);
        }
    }
}

/// Find the previous char boundary before `pos` in `s`.
pub(super) fn prev_char_boundary(s: &str, pos: usize) -> usize {
    s[..pos].char_indices().next_back().map_or(0, |(i, _)| i)
}

/// Find the next char boundary after `pos` in `s`.
pub(super) fn next_char_boundary(s: &str, pos: usize) -> usize {
    let mut iter = s[pos..].char_indices();
    let _ = iter.next(); // skip current char
    iter.next().map_or(s.len(), |(i, _)| pos + i)
}

/// Find the word boundary to the left of `pos` (skip whitespace, then word chars).
pub(super) fn search_word_boundary_left(s: &str, pos: usize) -> usize {
    let prefix = &s[..pos];
    let mut chars = prefix.char_indices().rev().peekable();
    // Skip trailing whitespace
    while chars.peek().is_some_and(|&(_, c)| c.is_whitespace()) {
        let _ = chars.next();
    }
    // Skip word chars
    let mut boundary = chars.peek().map_or(0, |&(i, _)| i);
    while let Some(&(i, c)) = chars.peek() {
        if c.is_whitespace() {
            boundary = next_char_boundary(s, i);
            break;
        }
        boundary = i;
        let _ = chars.next();
    }
    boundary
}

/// Find the word boundary to the right of `pos` (skip word chars, then whitespace).
pub(super) fn search_word_boundary_right(s: &str, pos: usize) -> usize {
    let suffix = &s[pos..];
    let mut chars = suffix.char_indices().peekable();
    // Skip word chars
    while chars.peek().is_some_and(|&(_, c)| !c.is_whitespace()) {
        let _ = chars.next();
    }
    // Skip whitespace
    while chars.peek().is_some_and(|&(_, c)| c.is_whitespace()) {
        let _ = chars.next();
    }
    chars.peek().map_or(s.len(), |&(i, _)| pos + i)
}
