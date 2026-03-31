use libghostty_vt::ffi;
use libghostty_vt::render::{CellIterator, RowIterator};
use libghostty_vt::style::{Style, StyleColor, Underline};

/// Color information extracted from the render state.
#[derive(Clone)]
pub struct RenderColors {
    pub foreground: (u8, u8, u8),
    pub background: (u8, u8, u8),
    pub cursor: Option<(u8, u8, u8)>,
    pub palette: [(u8, u8, u8); 256],
}

/// Cursor visual style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Bar,
    Block,
    Underline,
    BlockHollow,
}

/// Cursor state from the render state.
pub struct CursorState {
    pub visible: bool,
    pub in_viewport: bool,
    pub x: u16,
    pub y: u16,
    pub style: CursorStyle,
    pub blinking: bool,
}

/// Boolean attributes for a terminal cell, stored as a bitfield to avoid
/// triggering the `clippy::struct_excessive_bools` lint.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CellStyleAttrs(u8);

impl CellStyleAttrs {
    pub const BOLD: u8 = 1 << 0;
    pub const ITALIC: u8 = 1 << 1;
    pub const FAINT: u8 = 1 << 2;
    pub const BLINK: u8 = 1 << 3;
    pub const INVERSE: u8 = 1 << 4;
    pub const INVISIBLE: u8 = 1 << 5;
    pub const STRIKETHROUGH: u8 = 1 << 6;
    pub const OVERLINE: u8 = 1 << 7;

    /// Create from a raw bitfield value.
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    pub const fn bold(self) -> bool {
        self.0 & Self::BOLD != 0
    }
    pub const fn italic(self) -> bool {
        self.0 & Self::ITALIC != 0
    }
    pub const fn faint(self) -> bool {
        self.0 & Self::FAINT != 0
    }
    pub const fn blink(self) -> bool {
        self.0 & Self::BLINK != 0
    }
    pub const fn inverse(self) -> bool {
        self.0 & Self::INVERSE != 0
    }
    pub const fn invisible(self) -> bool {
        self.0 & Self::INVISIBLE != 0
    }
    pub const fn strikethrough(self) -> bool {
        self.0 & Self::STRIKETHROUGH != 0
    }
    pub const fn overline(self) -> bool {
        self.0 & Self::OVERLINE != 0
    }
}

/// Cell style resolved from the render state.
#[derive(Debug, Clone)]
pub struct CellStyle {
    pub fg_tag: u32,
    pub fg_palette: u8,
    pub fg_rgb: (u8, u8, u8),
    pub bg_tag: u32,
    pub bg_palette: u8,
    pub bg_rgb: (u8, u8, u8),
    pub underline_color_tag: u32,
    pub underline_color_palette: u8,
    pub underline_color_rgb: (u8, u8, u8),
    pub attrs: CellStyleAttrs,
    pub underline: i32,
}

/// Safe wrapper around `libghostty_vt::RenderState` and its associated
/// iterators. Pre-allocates row iterator and cell iterator once and reuses
/// them across every call to [`for_each_cell`].
pub struct RenderState {
    inner: libghostty_vt::RenderState<'static>,
    row_iter: RowIterator<'static>,
    cell_iter: CellIterator<'static>,
}

unsafe impl Send for RenderState {}

impl RenderState {
    /// Create a new render state with pre-allocated iterator and cells.
    pub fn new() -> Result<Self, &'static str> {
        let inner =
            libghostty_vt::RenderState::new().map_err(|_| "failed to create render state")?;
        let row_iter = RowIterator::new().map_err(|_| "failed to create row iterator")?;
        let cell_iter = CellIterator::new().map_err(|_| "failed to create cell iterator")?;

        Ok(Self {
            inner,
            row_iter,
            cell_iter,
        })
    }

    /// Update the render state from a terminal.
    pub fn update(
        &mut self,
        terminal: &libghostty_vt::Terminal<'static, '_>,
    ) -> Result<(), &'static str> {
        let _ = self
            .inner
            .update(terminal)
            .map_err(|_| "failed to update render state")?;
        Ok(())
    }

    /// Return the current dirty state value.
    pub fn dirty(&self) -> u32 {
        // We need a snapshot to query dirty state, but we can't get one
        // without &mut self and a terminal. Use the raw FFI directly.
        let mut dirty: ffi::GhosttyRenderStateDirty = 0;
        unsafe {
            let _ = ffi::ghostty_render_state_get(
                self.inner_raw(),
                ffi::GhosttyRenderStateData_GHOSTTY_RENDER_STATE_DATA_DIRTY,
                std::ptr::from_mut(&mut dirty).cast(),
            );
        }
        dirty
    }

    /// Reset the dirty state to `DIRTY_FALSE`.
    pub fn clear_dirty(&mut self) {
        let clean = ffi::GhosttyRenderStateDirty_GHOSTTY_RENDER_STATE_DIRTY_FALSE;
        unsafe {
            let _ = ffi::ghostty_render_state_set(
                self.inner_raw(),
                ffi::GhosttyRenderStateOption_GHOSTTY_RENDER_STATE_OPTION_DIRTY,
                std::ptr::from_ref(&clean).cast(),
            );
        }
    }

    /// Get the grid dimensions as `(cols, rows)`.
    pub fn dimensions(&self) -> (u16, u16) {
        let mut cols: u16 = 0;
        let mut rows: u16 = 0;
        unsafe {
            let _ = ffi::ghostty_render_state_get(
                self.inner_raw(),
                ffi::GhosttyRenderStateData_GHOSTTY_RENDER_STATE_DATA_COLS,
                std::ptr::from_mut(&mut cols).cast(),
            );
            let _ = ffi::ghostty_render_state_get(
                self.inner_raw(),
                ffi::GhosttyRenderStateData_GHOSTTY_RENDER_STATE_DATA_ROWS,
                std::ptr::from_mut(&mut rows).cast(),
            );
        }
        (cols, rows)
    }

    /// Get the color information from the render state.
    pub fn colors(&self) -> RenderColors {
        let mut c = unsafe {
            std::mem::MaybeUninit::<ffi::GhosttyRenderStateColors>::zeroed().assume_init()
        };
        c.size = size_of::<ffi::GhosttyRenderStateColors>();
        unsafe {
            let _ = ffi::ghostty_render_state_colors_get(self.inner_raw(), &raw mut c);
        }

        let mut fg = (c.foreground.r, c.foreground.g, c.foreground.b);
        let bg = (c.background.r, c.background.g, c.background.b);

        // Fall back to white-on-black when both report (0,0,0).
        if fg == (0, 0, 0) && bg == (0, 0, 0) {
            fg = (255, 255, 255);
        }

        let cursor = if c.cursor_has_value {
            Some((c.cursor.r, c.cursor.g, c.cursor.b))
        } else {
            None
        };

        let mut palette = [(0u8, 0u8, 0u8); 256];
        for (dst, p) in palette.iter_mut().zip(c.palette.iter()) {
            *dst = (p.r, p.g, p.b);
        }

        RenderColors {
            foreground: fg,
            background: bg,
            cursor,
            palette,
        }
    }

    /// Get the cursor state.
    pub fn cursor(&self) -> CursorState {
        let mut visible = false;
        let mut in_viewport = false;
        let mut cx: u16 = 0;
        let mut cy: u16 = 0;
        let mut vstyle: ffi::GhosttyRenderStateCursorVisualStyle = 0;
        let mut blinking = false;

        unsafe {
            let raw = self.inner_raw();
            let _ = ffi::ghostty_render_state_get(
                raw,
                ffi::GhosttyRenderStateData_GHOSTTY_RENDER_STATE_DATA_CURSOR_VISIBLE,
                std::ptr::from_mut(&mut visible).cast(),
            );
            let _ = ffi::ghostty_render_state_get(
                raw,
                ffi::GhosttyRenderStateData_GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE,
                std::ptr::from_mut(&mut in_viewport).cast(),
            );
            let _ = ffi::ghostty_render_state_get(
                raw,
                ffi::GhosttyRenderStateData_GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X,
                std::ptr::from_mut(&mut cx).cast(),
            );
            let _ = ffi::ghostty_render_state_get(
                raw,
                ffi::GhosttyRenderStateData_GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y,
                std::ptr::from_mut(&mut cy).cast(),
            );
            let _ = ffi::ghostty_render_state_get(
                raw,
                ffi::GhosttyRenderStateData_GHOSTTY_RENDER_STATE_DATA_CURSOR_VISUAL_STYLE,
                std::ptr::from_mut(&mut vstyle).cast(),
            );
            let _ = ffi::ghostty_render_state_get(
                raw,
                ffi::GhosttyRenderStateData_GHOSTTY_RENDER_STATE_DATA_CURSOR_BLINKING,
                std::ptr::from_mut(&mut blinking).cast(),
            );
        }

        let style = match vstyle {
            ffi::GhosttyRenderStateCursorVisualStyle_GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BAR => CursorStyle::Bar,
            ffi::GhosttyRenderStateCursorVisualStyle_GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_UNDERLINE => CursorStyle::Underline,
            ffi::GhosttyRenderStateCursorVisualStyle_GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK_HOLLOW => CursorStyle::BlockHollow,
            // Block is the default for unrecognized values
            _ => CursorStyle::Block,
        };

        CursorState {
            visible,
            in_viewport,
            x: cx,
            y: cy,
            style,
            blinking,
        }
    }

    /// Get a list of dirty row indices (0-based), stack-allocated.
    pub fn dirty_row_indices(&mut self, total_rows: u16) -> DirtyRows {
        let mut result = DirtyRows::new();
        unsafe {
            let r = ffi::ghostty_render_state_get(
                self.inner_raw(),
                ffi::GhosttyRenderStateData_GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR,
                std::ptr::from_mut(&mut self.row_iter).cast(),
            );
            if r != ffi::GhosttyResult_GHOSTTY_SUCCESS {
                return result;
            }
        }

        let mut row_idx: u16 = 0;

        while unsafe { ffi::ghostty_render_state_row_iterator_next(self.row_iter_raw()) } {
            let mut is_dirty = false;
            unsafe {
                let _ = ffi::ghostty_render_state_row_get(
                    self.row_iter_raw(),
                    ffi::GhosttyRenderStateRowData_GHOSTTY_RENDER_STATE_ROW_DATA_DIRTY,
                    std::ptr::from_mut(&mut is_dirty).cast(),
                );
            }
            if is_dirty {
                result.push(row_idx);
            }
            row_idx += 1;
            if row_idx >= total_rows {
                break;
            }
        }

        result
    }

    /// Iterate over every row and cell in the current render state.
    pub fn for_each_cell<F>(&mut self, mut callback: F)
    where
        F: FnMut(usize, usize, &[u32], &CellStyle, bool),
    {
        self.for_each_cell_filtered(false, &mut callback);
    }

    /// Iterate only over dirty rows.
    pub fn for_each_dirty_cell<F>(&mut self, callback: &mut F)
    where
        F: FnMut(usize, usize, &[u32], &CellStyle, bool),
    {
        self.for_each_cell_filtered(true, callback);
    }

    pub(crate) fn for_each_cell_filtered<F>(&mut self, dirty_only: bool, callback: &mut F)
    where
        F: FnMut(usize, usize, &[u32], &CellStyle, bool),
    {
        unsafe {
            let result = ffi::ghostty_render_state_get(
                self.inner_raw(),
                ffi::GhosttyRenderStateData_GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR,
                std::ptr::from_mut(&mut self.row_iter).cast(),
            );
            if result != ffi::GhosttyResult_GHOSTTY_SUCCESS {
                return;
            }
        }

        let mut row_idx: usize = 0;

        while unsafe { ffi::ghostty_render_state_row_iterator_next(self.row_iter_raw()) } {
            if dirty_only {
                let mut is_dirty = false;
                unsafe {
                    let _ = ffi::ghostty_render_state_row_get(
                        self.row_iter_raw(),
                        ffi::GhosttyRenderStateRowData_GHOSTTY_RENDER_STATE_ROW_DATA_DIRTY,
                        std::ptr::from_mut(&mut is_dirty).cast(),
                    );
                }
                if !is_dirty {
                    row_idx += 1;
                    continue;
                }
            }
            self.process_row(row_idx, callback);
            row_idx += 1;
        }
    }

    fn process_row<F>(&mut self, row_idx: usize, callback: &mut F)
    where
        F: FnMut(usize, usize, &[u32], &CellStyle, bool),
    {
        let result = unsafe {
            ffi::ghostty_render_state_row_get(
                self.row_iter_raw(),
                ffi::GhosttyRenderStateRowData_GHOSTTY_RENDER_STATE_ROW_DATA_CELLS,
                std::ptr::from_mut(&mut self.cell_iter).cast(),
            )
        };
        if result != ffi::GhosttyResult_GHOSTTY_SUCCESS {
            return;
        }

        let mut col_idx: usize = 0;

        while unsafe { ffi::ghostty_render_state_row_cells_next(self.cell_iter_raw()) } {
            Self::process_cell_raw(self.cell_iter_raw(), row_idx, col_idx, callback);
            col_idx += 1;
        }

        // Clear the per-row dirty flag.
        let clean = false;
        unsafe {
            let _ = ffi::ghostty_render_state_row_set(
                self.row_iter_raw(),
                ffi::GhosttyRenderStateRowOption_GHOSTTY_RENDER_STATE_ROW_OPTION_DIRTY,
                std::ptr::from_ref(&clean).cast(),
            );
        }
    }

    fn process_cell_raw<F>(
        row_cells: ffi::GhosttyRenderStateRowCells_ptr,
        row_idx: usize,
        col_idx: usize,
        callback: &mut F,
    ) where
        F: FnMut(usize, usize, &[u32], &CellStyle, bool),
    {
        let mut grapheme_len: u32 = 0;
        unsafe {
            let _ = ffi::ghostty_render_state_row_cells_get(
                row_cells,
                ffi::GhosttyRenderStateRowCellsData_GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN,
                std::ptr::from_mut(&mut grapheme_len).cast(),
            );
        }

        let mut raw_cell: ffi::GhosttyCell = 0;
        let mut wide: ffi::GhosttyCellWide = 0;
        unsafe {
            let _ = ffi::ghostty_render_state_row_cells_get(
                row_cells,
                ffi::GhosttyRenderStateRowCellsData_GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
                std::ptr::from_mut(&mut raw_cell).cast(),
            );
            let _ = ffi::ghostty_cell_get(
                raw_cell,
                ffi::GhosttyCellData_GHOSTTY_CELL_DATA_WIDE,
                std::ptr::from_mut(&mut wide).cast(),
            );
        }

        let is_wide = wide == ffi::GhosttyCellWide_GHOSTTY_CELL_WIDE_WIDE;
        let is_spacer_tail = wide == ffi::GhosttyCellWide_GHOSTTY_CELL_WIDE_SPACER_TAIL;

        if grapheme_len > 0 && !is_spacer_tail {
            emit_grapheme_cell(row_cells, row_idx, col_idx, grapheme_len, is_wide, callback);
        } else {
            emit_empty_cell(row_idx, col_idx, raw_cell, callback);
        }
    }

    /// Get the raw render state pointer for direct FFI access.
    const fn inner_raw(&self) -> ffi::GhosttyRenderState_ptr {
        // Access the internal Object's raw pointer through the struct layout.
        // The RenderState<'alloc> wraps Object<'alloc, GhosttyRenderState> which
        // contains the raw pointer as its first field.
        //
        // SAFETY: We know the memory layout — RenderState wraps a single Object
        // which wraps a raw pointer as its first field.
        unsafe { *std::ptr::from_ref(&self.inner).cast::<ffi::GhosttyRenderState_ptr>() }
    }

    const fn row_iter_raw(&self) -> ffi::GhosttyRenderStateRowIterator_ptr {
        unsafe {
            *std::ptr::from_ref(&self.row_iter).cast::<ffi::GhosttyRenderStateRowIterator_ptr>()
        }
    }

    const fn cell_iter_raw(&self) -> ffi::GhosttyRenderStateRowCells_ptr {
        unsafe {
            *std::ptr::from_ref(&self.cell_iter).cast::<ffi::GhosttyRenderStateRowCells_ptr>()
        }
    }
}

/// Read grapheme codepoints and style for a non-empty cell.
fn emit_grapheme_cell<F>(
    row_cells: ffi::GhosttyRenderStateRowCells_ptr,
    row_idx: usize,
    col_idx: usize,
    grapheme_len: u32,
    is_wide: bool,
    callback: &mut F,
) where
    F: FnMut(usize, usize, &[u32], &CellStyle, bool),
{
    let len = usize::try_from(grapheme_len.min(16)).unwrap_or(16);
    let mut codepoints = [0u32; 16];
    unsafe {
        let _ = ffi::ghostty_render_state_row_cells_get(
            row_cells,
            ffi::GhosttyRenderStateRowCellsData_GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF,
            codepoints.as_mut_ptr().cast(),
        );
    }

    let mut style_raw =
        unsafe { std::mem::MaybeUninit::<ffi::GhosttyStyle>::zeroed().assume_init() };
    style_raw.size = size_of::<ffi::GhosttyStyle>();
    unsafe {
        let _ = ffi::ghostty_render_state_row_cells_get(
            row_cells,
            ffi::GhosttyRenderStateRowCellsData_GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
            std::ptr::from_mut(&mut style_raw).cast(),
        );
    }

    let cell_style = convert_style_raw(&style_raw);
    let cp_slice = codepoints.get(..len).unwrap_or(&codepoints);
    callback(row_idx, col_idx, cp_slice, &cell_style, is_wide);
}

/// Handle an empty cell that may still carry a background-only color.
fn emit_empty_cell<F>(row_idx: usize, col_idx: usize, raw_cell: ffi::GhosttyCell, callback: &mut F)
where
    F: FnMut(usize, usize, &[u32], &CellStyle, bool),
{
    let mut content_tag: ffi::GhosttyCellContentTag = 0;
    unsafe {
        let _ = ffi::ghostty_cell_get(
            raw_cell,
            ffi::GhosttyCellData_GHOSTTY_CELL_DATA_CONTENT_TAG,
            std::ptr::from_mut(&mut content_tag).cast(),
        );
    }

    let cell_style = match content_tag {
        ffi::GhosttyCellContentTag_GHOSTTY_CELL_CONTENT_BG_COLOR_PALETTE => {
            let mut idx: u8 = 0;
            unsafe {
                let _ = ffi::ghostty_cell_get(
                    raw_cell,
                    ffi::GhosttyCellData_GHOSTTY_CELL_DATA_COLOR_PALETTE,
                    std::ptr::from_mut(&mut idx).cast(),
                );
            }
            let mut s = default_cell_style();
            s.bg_tag = ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE;
            s.bg_palette = idx;
            s
        }
        ffi::GhosttyCellContentTag_GHOSTTY_CELL_CONTENT_BG_COLOR_RGB => {
            let mut rgb =
                unsafe { std::mem::MaybeUninit::<ffi::GhosttyColorRgb>::zeroed().assume_init() };
            unsafe {
                let _ = ffi::ghostty_cell_get(
                    raw_cell,
                    ffi::GhosttyCellData_GHOSTTY_CELL_DATA_COLOR_RGB,
                    std::ptr::from_mut(&mut rgb).cast(),
                );
            }
            let mut s = default_cell_style();
            s.bg_tag = ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_RGB;
            s.bg_rgb = (rgb.r, rgb.g, rgb.b);
            s
        }
        _ => default_cell_style(),
    };

    callback(row_idx, col_idx, &[], &cell_style, false);
}

/// Return a [`CellStyle`] with all fields at their canonical default values.
pub(crate) fn default_cell_style() -> CellStyle {
    let mut raw = std::mem::MaybeUninit::<ffi::GhosttyStyle>::uninit();
    unsafe {
        ffi::ghostty_style_default(raw.as_mut_ptr());
    }
    convert_style_raw(unsafe { &*raw.as_ptr() })
}

/// Convert a raw `GhosttyStyle` into our `CellStyle`.
const fn convert_style_raw(s: &ffi::GhosttyStyle) -> CellStyle {
    CellStyle {
        fg_tag: s.fg_color.tag,
        fg_palette: unsafe { s.fg_color.value.palette },
        fg_rgb: unsafe {
            (
                s.fg_color.value.rgb.r,
                s.fg_color.value.rgb.g,
                s.fg_color.value.rgb.b,
            )
        },
        bg_tag: s.bg_color.tag,
        bg_palette: unsafe { s.bg_color.value.palette },
        bg_rgb: unsafe {
            (
                s.bg_color.value.rgb.r,
                s.bg_color.value.rgb.g,
                s.bg_color.value.rgb.b,
            )
        },
        underline_color_tag: s.underline_color.tag,
        underline_color_palette: unsafe { s.underline_color.value.palette },
        underline_color_rgb: unsafe {
            (
                s.underline_color.value.rgb.r,
                s.underline_color.value.rgb.g,
                s.underline_color.value.rgb.b,
            )
        },
        attrs: CellStyleAttrs::from_bits(
            if s.bold { CellStyleAttrs::BOLD } else { 0 }
                | if s.italic { CellStyleAttrs::ITALIC } else { 0 }
                | if s.faint { CellStyleAttrs::FAINT } else { 0 }
                | if s.blink { CellStyleAttrs::BLINK } else { 0 }
                | if s.inverse {
                    CellStyleAttrs::INVERSE
                } else {
                    0
                }
                | if s.invisible {
                    CellStyleAttrs::INVISIBLE
                } else {
                    0
                }
                | if s.strikethrough {
                    CellStyleAttrs::STRIKETHROUGH
                } else {
                    0
                }
                | if s.overline {
                    CellStyleAttrs::OVERLINE
                } else {
                    0
                },
        ),
        underline: s.underline,
    }
}

/// Convert a `libghostty_vt::style::Style` into our `CellStyle`.
pub const fn convert_style(s: &Style) -> CellStyle {
    let (fg_tag, fg_palette, fg_rgb) = convert_style_color(s.fg_color);
    let (bg_tag, bg_palette, bg_rgb) = convert_style_color(s.bg_color);
    let (ul_tag, ul_palette, ul_rgb) = convert_style_color(s.underline_color);

    CellStyle {
        fg_tag,
        fg_palette,
        fg_rgb,
        bg_tag,
        bg_palette,
        bg_rgb,
        underline_color_tag: ul_tag,
        underline_color_palette: ul_palette,
        underline_color_rgb: ul_rgb,
        attrs: CellStyleAttrs::from_bits(
            if s.bold { CellStyleAttrs::BOLD } else { 0 }
                | if s.italic { CellStyleAttrs::ITALIC } else { 0 }
                | if s.faint { CellStyleAttrs::FAINT } else { 0 }
                | if s.blink { CellStyleAttrs::BLINK } else { 0 }
                | if s.inverse {
                    CellStyleAttrs::INVERSE
                } else {
                    0
                }
                | if s.invisible {
                    CellStyleAttrs::INVISIBLE
                } else {
                    0
                }
                | if s.strikethrough {
                    CellStyleAttrs::STRIKETHROUGH
                } else {
                    0
                }
                | if s.overline {
                    CellStyleAttrs::OVERLINE
                } else {
                    0
                },
        ),
        underline: match s.underline {
            Underline::Single => 1,
            Underline::Double => 2,
            Underline::Curly => 3,
            Underline::Dotted => 4,
            Underline::Dashed => 5,
            // None and any future variants default to no underline
            _ => 0,
        },
    }
}

const fn convert_style_color(c: StyleColor) -> (u32, u8, (u8, u8, u8)) {
    match c {
        StyleColor::None => (
            ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_NONE,
            0,
            (0, 0, 0),
        ),
        StyleColor::Palette(idx) => (
            ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE,
            idx.0,
            (0, 0, 0),
        ),
        StyleColor::Rgb(rgb) => (
            ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_RGB,
            0,
            (rgb.r, rgb.g, rgb.b),
        ),
    }
}

/// Resolve a style color to a concrete RGB tuple.
#[inline]
pub fn resolve_color(
    tag: u32,
    palette_idx: u8,
    rgb: (u8, u8, u8),
    colors: &RenderColors,
    fallback: (u8, u8, u8),
) -> (u8, u8, u8) {
    match tag {
        t if t == ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_RGB => rgb,
        t if t == ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE => colors
            .palette
            .get(usize::from(palette_idx))
            .copied()
            .unwrap_or(fallback),
        _ => fallback,
    }
}

/// Stack-allocated list of dirty row indices (max 256 rows).
pub struct DirtyRows {
    buf: [u16; 256],
    len: u16,
}

impl DirtyRows {
    const fn new() -> Self {
        Self {
            buf: [0; 256],
            len: 0,
        }
    }

    fn push(&mut self, row: u16) {
        let idx = usize::from(self.len);
        if let Some(slot) = self.buf.get_mut(idx) {
            *slot = row;
            self.len += 1;
        }
    }

    pub fn as_slice(&self) -> &[u16] {
        let len = usize::from(self.len);
        self.buf.get(..len).unwrap_or(&[])
    }

    pub const fn len(&self) -> usize {
        self.len as usize
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
