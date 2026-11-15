use cosmic_text::{
    Action as CosmicAction, Attrs, Buffer, Cursor as CosmicCursor, Edit as _,
    Editor as CosmicEditor, Family, FontSystem, Metrics, Motion as CosmicMotion,
    Selection as CosmicSelection, Shaping, Wrap,
};
use iced::advanced::clipboard::{self, Clipboard};
use iced::advanced::input_method::{self, InputMethod};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::text::{self, Editor as _, Wrapping};
use iced::advanced::widget::{self, Widget, operation};
use iced::advanced::{Shell, mouse};
use iced::keyboard::{self, key};
use iced::widget::text_editor::{Catalog, Status, Style, StyleFn};
use iced::window;
use iced::{Element, Event, Font, Length, Padding, Pixels, Point, Rectangle, Size, Vector};
use langhian_core::detect_direction;
use std::borrow::Cow;
use std::cell::RefCell;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

static FONT_SYSTEM: OnceLock<Mutex<FontSystem>> = OnceLock::new();

fn font_system() -> &'static Mutex<FontSystem> {
    FONT_SYSTEM.get_or_init(|| {
        let mut fs = FontSystem::new();
        for bytes in crate::ui::theme::bundled_fonts() {
            fs.db_mut().load_font_data(bytes.to_vec());
        }
        Mutex::new(fs)
    })
}

fn to_cosmic_attrs(font: Font) -> Attrs<'static> {
    let family = match font.family {
        iced::font::Family::Name(name) => Family::Name(name),
        iced::font::Family::SansSerif => Family::SansSerif,
        iced::font::Family::Serif => Family::Serif,
        iced::font::Family::Cursive => Family::Cursive,
        iced::font::Family::Fantasy => Family::Fantasy,
        iced::font::Family::Monospace => Family::Monospace,
    };
    let weight = cosmic_text::Weight(match font.weight {
        iced::font::Weight::Thin => 100,
        iced::font::Weight::ExtraLight => 200,
        iced::font::Weight::Light => 300,
        iced::font::Weight::Normal => 400,
        iced::font::Weight::Medium => 500,
        iced::font::Weight::Semibold => 600,
        iced::font::Weight::Bold => 700,
        iced::font::Weight::ExtraBold => 800,
        iced::font::Weight::Black => 900,
    });
    let style = match font.style {
        iced::font::Style::Normal => cosmic_text::Style::Normal,
        iced::font::Style::Italic => cosmic_text::Style::Italic,
        iced::font::Style::Oblique => cosmic_text::Style::Oblique,
    };
    Attrs::new().family(family).weight(weight).style(style)
}

fn to_cosmic_wrap(wrapping: Wrapping) -> Wrap {
    match wrapping {
        Wrapping::None => Wrap::None,
        Wrapping::Word => Wrap::Word,
        Wrapping::Glyph => Wrap::Glyph,
        Wrapping::WordOrGlyph => Wrap::WordOrGlyph,
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    Click {
        content_pos: Point,
        modifiers_shift: bool,
    },
    DoubleClick {
        content_pos: Point,
    },
    TripleClick {
        content_pos: Point,
    },
    Drag {
        content_pos: Point,
    },
    Move {
        motion: Motion,
        select: bool,
    },
    SelectAll,
    Scroll {
        delta_y: f32,
    },
    Edit(Edit),
}

impl Action {
    pub fn is_edit(&self) -> bool {
        matches!(self, Self::Edit(_))
    }
}

#[derive(Debug, Clone)]
pub enum Edit {
    Insert(char),
    Paste(Arc<String>),
    Enter,
    Backspace,
    Delete,
    Cut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    Home,
    End,
    PageUp,
    PageDown,
    DocumentStart,
    DocumentEnd,
}

impl From<Motion> for CosmicMotion {
    fn from(m: Motion) -> Self {
        match m {
            Motion::Left => CosmicMotion::Left,
            Motion::Right => CosmicMotion::Right,
            Motion::Up => CosmicMotion::Up,
            Motion::Down => CosmicMotion::Down,
            Motion::WordLeft => CosmicMotion::LeftWord,
            Motion::WordRight => CosmicMotion::RightWord,
            Motion::Home => CosmicMotion::Home,
            Motion::End => CosmicMotion::End,
            Motion::PageUp => CosmicMotion::PageUp,
            Motion::PageDown => CosmicMotion::PageDown,
            Motion::DocumentStart => CosmicMotion::BufferStart,
            Motion::DocumentEnd => CosmicMotion::BufferEnd,
        }
    }
}

struct Internal<Renderer: text::Renderer> {
    cosmic_editor: CosmicEditor<'static>,
    render_editor: Renderer::Editor,
    cached_text: String,
    dirty_render: bool,
    scroll_y: f32,
    content_height: f32,
    viewport_height: f32,
    layout_width: f32,
    font: Font,
    size: f32,
    line_height: text::LineHeight,
    wrapping: Wrapping,
}

impl<Renderer> std::fmt::Debug for Internal<Renderer>
where
    Renderer: text::Renderer,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Internal")
            .field("cached_text_len", &self.cached_text.len())
            .field("scroll_y", &self.scroll_y)
            .field("content_height", &self.content_height)
            .finish()
    }
}

pub struct Content<Renderer: text::Renderer = iced::Renderer>(RefCell<Internal<Renderer>>);

impl<Renderer> std::fmt::Debug for Content<Renderer>
where
    Renderer: text::Renderer,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Content").field(&self.0.borrow()).finish()
    }
}

impl<Renderer> Default for Content<Renderer>
where
    Renderer: text::Renderer<Font = Font>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Renderer> Content<Renderer>
where
    Renderer: text::Renderer<Font = Font>,
{
    pub fn new() -> Self {
        Self::with_text("")
    }

    pub fn with_text(text: &str) -> Self {
        let font = Font::DEFAULT;
        let size: f32 = 14.0;
        let line_height_px = (size * 1.35).ceil();
        let mut buffer = Buffer::new_empty(Metrics::new(size, line_height_px));

        {
            let mut fs = font_system().lock().expect("lock font system");
            buffer.set_text(
                &mut fs,
                text,
                &to_cosmic_attrs(font),
                Shaping::Advanced,
                None,
            );
            buffer.set_wrap(&mut fs, Wrap::Word);
        }

        let cosmic_editor = CosmicEditor::new(buffer);
        let render_editor = Renderer::Editor::with_text(text);

        Self(RefCell::new(Internal {
            cosmic_editor,
            render_editor,
            cached_text: normalize_line_endings(text),
            dirty_render: true,
            scroll_y: 0.0,
            content_height: line_height_px,
            viewport_height: 0.0,
            layout_width: 0.0,
            font,
            size,
            line_height: text::LineHeight::Relative(1.35),
            wrapping: Wrapping::Word,
        }))
    }

    pub fn text(&self) -> String {
        let mut out = self.0.borrow().cached_text.clone();
        if !out.is_empty() && !out.ends_with('\n') {
            return out;
        }
        out.shrink_to_fit();
        out
    }

    pub fn is_empty(&self) -> bool {
        self.0.borrow().cached_text.is_empty()
    }

    pub fn selection(&self) -> Option<String> {
        self.0.borrow().cosmic_editor.copy_selection()
    }

    pub fn line_count(&self) -> usize {
        self.0
            .borrow()
            .cosmic_editor
            .with_buffer(|b| b.lines.len().max(1))
    }

    pub fn perform(&mut self, action: Action) {
        let mut internal = self.0.borrow_mut();
        let mut fs = font_system().lock().expect("lock font system");
        let mut text_mutated = false;
        let mut ensure_caret_visible = false;

        match action {
            Action::Click {
                content_pos,
                modifiers_shift,
            } => {
                let hit_cursor = internal
                    .cosmic_editor
                    .with_buffer(|b| rtl_aware_hit(b, content_pos.x, content_pos.y));
                if let Some(target) = hit_cursor {
                    if modifiers_shift {
                        if internal.cosmic_editor.selection() == CosmicSelection::None {
                            let cur = internal.cosmic_editor.cursor();
                            internal
                                .cosmic_editor
                                .set_selection(CosmicSelection::Normal(cur));
                        }
                        internal.cosmic_editor.set_cursor(target);
                    } else {
                        internal.cosmic_editor.set_selection(CosmicSelection::None);
                        internal.cosmic_editor.set_cursor(target);
                    }
                    internal
                        .cosmic_editor
                        .with_buffer_mut(|b| b.set_redraw(true));
                }
                ensure_caret_visible = true;
            }
            Action::DoubleClick { content_pos } => {
                if let Some(target) = internal
                    .cosmic_editor
                    .with_buffer(|b| rtl_aware_hit(b, content_pos.x, content_pos.y))
                {
                    internal.cosmic_editor.set_cursor(target);
                    internal
                        .cosmic_editor
                        .set_selection(CosmicSelection::Word(target));
                    internal
                        .cosmic_editor
                        .with_buffer_mut(|b| b.set_redraw(true));
                }
            }
            Action::TripleClick { content_pos } => {
                if let Some(target) = internal
                    .cosmic_editor
                    .with_buffer(|b| rtl_aware_hit(b, content_pos.x, content_pos.y))
                {
                    internal.cosmic_editor.set_cursor(target);
                    internal
                        .cosmic_editor
                        .set_selection(CosmicSelection::Line(target));
                    internal
                        .cosmic_editor
                        .with_buffer_mut(|b| b.set_redraw(true));
                }
            }
            Action::Drag { content_pos } => {
                if internal.cosmic_editor.selection() == CosmicSelection::None {
                    let cur = internal.cosmic_editor.cursor();
                    internal
                        .cosmic_editor
                        .set_selection(CosmicSelection::Normal(cur));
                }
                if let Some(target) = internal
                    .cosmic_editor
                    .with_buffer(|b| rtl_aware_hit(b, content_pos.x, content_pos.y))
                {
                    internal.cosmic_editor.set_cursor(target);
                    internal
                        .cosmic_editor
                        .with_buffer_mut(|b| b.set_redraw(true));
                }
                ensure_caret_visible = true;
            }
            Action::Move { motion, select } => {
                if select {
                    if internal.cosmic_editor.selection() == CosmicSelection::None {
                        let cur = internal.cosmic_editor.cursor();
                        internal
                            .cosmic_editor
                            .set_selection(CosmicSelection::Normal(cur));
                    }
                    internal
                        .cosmic_editor
                        .action(&mut fs, CosmicAction::Motion(motion.into()));
                    if let Some((s, e)) = internal.cosmic_editor.selection_bounds()
                        && s == e
                    {
                        internal.cosmic_editor.set_selection(CosmicSelection::None);
                    }
                } else if let Some((start, end)) = internal.cosmic_editor.selection_bounds() {
                    internal.cosmic_editor.set_selection(CosmicSelection::None);
                    match motion {
                        Motion::Home
                        | Motion::End
                        | Motion::DocumentStart
                        | Motion::DocumentEnd
                        | Motion::Up
                        | Motion::Down
                        | Motion::PageUp
                        | Motion::PageDown => {
                            internal
                                .cosmic_editor
                                .action(&mut fs, CosmicAction::Motion(motion.into()));
                        }
                        Motion::Left | Motion::WordLeft => {
                            let is_rtl_line = internal
                                .cosmic_editor
                                .with_buffer(|b| line_is_rtl(b, start.line));
                            internal.cosmic_editor.set_cursor(if is_rtl_line {
                                end
                            } else {
                                start
                            });
                        }
                        Motion::Right | Motion::WordRight => {
                            let is_rtl_line = internal
                                .cosmic_editor
                                .with_buffer(|b| line_is_rtl(b, start.line));
                            internal.cosmic_editor.set_cursor(if is_rtl_line {
                                start
                            } else {
                                end
                            });
                        }
                    }
                } else {
                    internal
                        .cosmic_editor
                        .action(&mut fs, CosmicAction::Motion(motion.into()));
                }
                ensure_caret_visible = true;
            }
            Action::SelectAll => {
                internal.cosmic_editor.set_cursor(CosmicCursor::new(0, 0));
                internal
                    .cosmic_editor
                    .set_selection(CosmicSelection::Normal(CosmicCursor::new(0, 0)));
                let last = internal.cosmic_editor.with_buffer(|b| {
                    let line_i = b.lines.len().saturating_sub(1);
                    let idx = b.lines.get(line_i).map_or(0, |l| l.text().len());
                    CosmicCursor::new(line_i, idx)
                });
                internal.cosmic_editor.set_cursor(last);
                internal
                    .cosmic_editor
                    .with_buffer_mut(|b| b.set_redraw(true));
            }
            Action::Scroll { delta_y } => {
                let max_scroll = (internal.content_height - internal.viewport_height).max(0.0);
                internal.scroll_y = (internal.scroll_y + delta_y).clamp(0.0, max_scroll);
            }
            Action::Edit(edit) => {
                match edit {
                    Edit::Insert(ch) => {
                        internal.cosmic_editor.delete_selection();
                        internal
                            .cosmic_editor
                            .action(&mut fs, CosmicAction::Insert(ch));
                        text_mutated = true;
                    }
                    Edit::Paste(s) => {
                        internal.cosmic_editor.delete_selection();
                        let normalized = normalize_line_endings(&s);
                        internal.cosmic_editor.insert_string(&normalized, None);
                        text_mutated = true;
                    }
                    Edit::Enter => {
                        internal.cosmic_editor.delete_selection();
                        internal.cosmic_editor.action(&mut fs, CosmicAction::Enter);
                        text_mutated = true;
                    }
                    Edit::Backspace => {
                        if !internal.cosmic_editor.delete_selection() {
                            internal
                                .cosmic_editor
                                .action(&mut fs, CosmicAction::Backspace);
                        }
                        text_mutated = true;
                    }
                    Edit::Delete => {
                        if !internal.cosmic_editor.delete_selection() {
                            internal.cosmic_editor.action(&mut fs, CosmicAction::Delete);
                        }
                        text_mutated = true;
                    }
                    Edit::Cut => {
                        if internal.cosmic_editor.delete_selection() {
                            text_mutated = true;
                        }
                    }
                }
                ensure_caret_visible = true;
            }
        }

        internal.cosmic_editor.shape_as_needed(&mut fs, false);

        if text_mutated {
            let latest = internal.cosmic_editor.with_buffer(extract_buffer_text);
            internal.cached_text = latest;
            internal.dirty_render = true;
            // Re-apply attrs cleanly to any newly created lines after Enter/Paste
            let attrs = to_cosmic_attrs(internal.font);
            internal.cosmic_editor.with_buffer_mut(|b| {
                for line in b.lines.iter_mut() {
                    let _ = line.set_attrs_list(cosmic_text::AttrsList::new(&attrs));
                }
            });
            internal.cosmic_editor.shape_as_needed(&mut fs, false);
        }

        internal.content_height = internal.cosmic_editor.with_buffer(measure_content_height);

        if ensure_caret_visible {
            let line_h = internal
                .cosmic_editor
                .with_buffer(|b| b.metrics().line_height);
            let cur = internal.cosmic_editor.cursor();
            let caret_y = internal
                .cosmic_editor
                .with_buffer(|b| find_caret_y(b, cur))
                .unwrap_or(0.0);
            let max_scroll = (internal.content_height - internal.viewport_height).max(0.0);
            if internal.viewport_height > 0.0 {
                if caret_y < internal.scroll_y {
                    internal.scroll_y = caret_y.clamp(0.0, max_scroll);
                } else if caret_y + line_h > internal.scroll_y + internal.viewport_height {
                    internal.scroll_y =
                        (caret_y + line_h - internal.viewport_height).clamp(0.0, max_scroll);
                } else {
                    internal.scroll_y = internal.scroll_y.clamp(0.0, max_scroll);
                }
            }
        }
    }
}

fn normalize_line_endings(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}

fn extract_buffer_text(buffer: &Buffer) -> String {
    let mut out = String::new();
    for (i, line) in buffer.lines.iter().enumerate() {
        out.push_str(line.text());
        if i + 1 < buffer.lines.len() {
            out.push('\n');
        }
    }
    out
}

fn line_is_rtl(buffer: &Buffer, line_i: usize) -> bool {
    if let Some(run) = buffer.layout_runs().find(|r| r.line_i == line_i) {
        return run.rtl;
    }
    buffer
        .lines
        .get(line_i)
        .is_some_and(|l| detect_direction(l.text(), None).is_rtl())
}

fn measure_content_height(buffer: &Buffer) -> f32 {
    let line_height = buffer.metrics().line_height;
    let mut max_bottom = line_height;
    for run in buffer.layout_runs() {
        let bottom = run.line_top + run.line_height;
        if bottom > max_bottom {
            max_bottom = bottom;
        }
    }
    max_bottom.max(line_height)
}

fn find_caret_y(buffer: &Buffer, cursor: CosmicCursor) -> Option<f32> {
    let mut last_y_for_line = None;
    for run in buffer.layout_runs() {
        if run.line_i == cursor.line {
            last_y_for_line = Some(run.line_top);
            if let Some(start_g) = run.glyphs.first().map(|g| g.start)
                && let Some(end_g) = run.glyphs.last().map(|g| g.end)
            {
                let lo = start_g.min(end_g);
                let hi = start_g.max(end_g);
                if cursor.index >= lo && cursor.index <= hi {
                    return Some(run.line_top);
                }
            } else {
                return Some(run.line_top);
            }
        }
    }
    last_y_for_line
}

/// RTL- and BiDi-aware hit testing using actual glyph X bounds (`glyph.x`, `glyph.w`, `glyph.level`).
/// Fixes `cosmic_text::Buffer::hit` empty-space clicks/drags on right-aligned RTL runs and wrapped paragraphs.
pub fn rtl_aware_hit(buffer: &Buffer, x: f32, y: f32) -> Option<CosmicCursor> {
    let mut first_run_line: Option<usize> = None;
    let mut last_run: Option<(usize, usize, Option<usize>)> = None;
    let mut prev_run_bottom: Option<f32> = None;

    for run in buffer.layout_runs() {
        if first_run_line.is_none() {
            first_run_line = Some(run.line_i);
            if y < run.line_top {
                return Some(hit_in_run(&run, x));
            }
        }
        let max_glyph_end = run.glyphs.iter().map(|g| g.end).max();
        last_run = Some((run.line_i, run.line_y as usize, max_glyph_end));

        if y >= run.line_top && y < run.line_top + run.line_height {
            return Some(hit_in_run(&run, x));
        }

        if let Some(prev_b) = prev_run_bottom
            && y >= prev_b
            && y < run.line_top
        {
            return Some(hit_in_run(&run, x));
        }
        prev_run_bottom = Some(run.line_top + run.line_height);
    }

    if let Some((line_i, _, max_end)) = last_run {
        let idx = max_end.unwrap_or_else(|| buffer.lines.get(line_i).map_or(0, |l| l.text().len()));
        return Some(CosmicCursor::new(line_i, idx));
    }

    first_run_line.map(|line_i| CosmicCursor::new(line_i, 0))
}

fn hit_in_run(run: &cosmic_text::LayoutRun<'_>, x: f32) -> CosmicCursor {
    if run.glyphs.is_empty() {
        return CosmicCursor::new(run.line_i, 0);
    }

    // Visually leftmost and rightmost glyphs regardless of logical glyph storage order.
    let min_glyph = run
        .glyphs
        .iter()
        .min_by(|a, b| a.x.total_cmp(&b.x))
        .expect("non-empty glyphs");
    let max_glyph = run
        .glyphs
        .iter()
        .max_by(|a, b| (a.x + a.w).total_cmp(&(b.x + b.w)))
        .expect("non-empty glyphs");

    if x <= min_glyph.x {
        let idx = if min_glyph.level.is_rtl() {
            min_glyph.end
        } else {
            min_glyph.start
        };
        return CosmicCursor::new(run.line_i, idx);
    }

    if x >= max_glyph.x + max_glyph.w {
        let idx = if max_glyph.level.is_rtl() {
            max_glyph.start
        } else {
            max_glyph.end
        };
        return CosmicCursor::new(run.line_i, idx);
    }

    // Direct glyph hit or nearest horizontal glyph in case of subpixel gaps.
    let target_glyph = run
        .glyphs
        .iter()
        .find(|g| x >= g.x && x <= g.x + g.w)
        .unwrap_or_else(|| {
            run.glyphs
                .iter()
                .min_by(|a, b| {
                    let da = ((a.x + a.w * 0.5) - x).abs();
                    let db = ((b.x + b.w * 0.5) - x).abs();
                    da.total_cmp(&db)
                })
                .expect("non-empty glyphs")
        });

    let cluster = &run.text[target_glyph.start..target_glyph.end];
    let graphemes: Vec<(usize, &str)> = cluster.grapheme_indices(true).collect();
    if graphemes.len() <= 1 || target_glyph.w <= 0.0 {
        let right_half = x >= target_glyph.x + target_glyph.w * 0.5;
        let idx = if right_half != target_glyph.level.is_rtl() {
            target_glyph.end
        } else {
            target_glyph.start
        };
        return CosmicCursor::new(run.line_i, idx);
    }

    let n = graphemes.len() as f32;
    let step_w = target_glyph.w / n;
    let is_rtl_glyph = target_glyph.level.is_rtl();
    for (gi, (byte_off, g_str)) in graphemes.iter().enumerate() {
        let cell_left = if is_rtl_glyph {
            target_glyph.x + target_glyph.w - (gi as f32 + 1.0) * step_w
        } else {
            target_glyph.x + (gi as f32) * step_w
        };
        let cell_right = cell_left + step_w;
        if x >= cell_left && x <= cell_right {
            let right_half = x >= cell_left + step_w * 0.5;
            let local = if right_half != is_rtl_glyph {
                byte_off + g_str.len()
            } else {
                *byte_off
            };
            return CosmicCursor::new(run.line_i, target_glyph.start + local);
        }
    }

    CosmicCursor::new(run.line_i, target_glyph.start)
}

/// Computes visual selection highlight rectangles (in buffer coordinates) from `layout_runs()`
/// using exact `glyph.x` / `glyph.w` / `glyph.level.is_rtl()`.
pub fn compute_selection_rects(
    buffer: &Buffer,
    start: CosmicCursor,
    end: CosmicCursor,
) -> Vec<Rectangle> {
    let mut rects = Vec::new();
    let buffer_w = buffer.size().0.unwrap_or(0.0);

    for run in buffer.layout_runs() {
        if run.line_i < start.line || run.line_i > end.line {
            continue;
        }
        let sel_from = if run.line_i == start.line {
            start.index
        } else {
            0
        };
        let sel_to = if run.line_i == end.line {
            end.index
        } else {
            run.text.len() + 1
        };

        if sel_to < sel_from {
            continue;
        }

        if run.glyphs.is_empty() {
            if run.line_i < end.line {
                let x = if run.rtl {
                    (buffer_w - 6.0).max(0.0)
                } else {
                    0.0
                };
                rects.push(Rectangle::new(
                    Point::new(x, run.line_top),
                    Size::new(6.0, run.line_height),
                ));
            }
            continue;
        }

        let mut spans: Vec<(f32, f32)> = Vec::new();
        for glyph in run.glyphs {
            let g_start = glyph.start;
            let g_end = glyph.end;
            let ov_start = g_start.max(sel_from);
            let ov_end = g_end.min(sel_to);
            if ov_start >= ov_end {
                continue;
            }

            if ov_start == g_start && ov_end == g_end {
                spans.push((glyph.x, glyph.x + glyph.w));
            } else {
                let g_len = (g_end - g_start).max(1) as f32;
                let f0 = (ov_start - g_start) as f32 / g_len;
                let f1 = (ov_end - g_start) as f32 / g_len;
                if glyph.level.is_rtl() {
                    let x1 = glyph.x + glyph.w * (1.0 - f0);
                    let x0 = glyph.x + glyph.w * (1.0 - f1);
                    spans.push((x0, x1));
                } else {
                    let x0 = glyph.x + glyph.w * f0;
                    let x1 = glyph.x + glyph.w * f1;
                    spans.push((x0, x1));
                }
            }
        }

        if spans.is_empty() {
            continue;
        }

        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<(f32, f32)> = Vec::with_capacity(spans.len());
        for (x0, x1) in spans {
            if let Some((_, prev_x1)) = merged.last_mut()
                && x0 <= *prev_x1 + 0.75
            {
                *prev_x1 = prev_x1.max(x1);
                continue;
            }
            merged.push((x0, x1));
        }

        for (x0, x1) in merged {
            let w = (x1 - x0).max(1.0);
            rects.push(Rectangle::new(
                Point::new(x0, run.line_top),
                Size::new(w, run.line_height),
            ));
        }
    }

    rects
}

/// Computes visual caret `(x, y, height)` in buffer coordinates using exact glyph coordinates.
pub fn compute_caret_rect(
    buffer: &Buffer,
    cursor: CosmicCursor,
    default_rtl: bool,
) -> Option<Rectangle> {
    let layout_width = buffer.size().0.unwrap_or(0.0);
    let mut fallback_run: Option<Rectangle> = None;

    for run in buffer.layout_runs() {
        if run.line_i != cursor.line {
            continue;
        }
        let line_rtl = if run.glyphs.is_empty() {
            buffer
                .lines
                .get(run.line_i)
                .map(|l| {
                    let t = l.text();
                    if t.trim().is_empty() {
                        default_rtl
                    } else {
                        detect_direction(t, None).is_rtl()
                    }
                })
                .unwrap_or(default_rtl)
        } else {
            run.rtl
        };

        if run.glyphs.is_empty() {
            let x = if line_rtl && layout_width > 1.5 {
                layout_width - 1.5
            } else {
                0.0
            };
            return Some(Rectangle::new(
                Point::new(x, run.line_top),
                Size::new(1.5, run.line_height),
            ));
        }

        let min_glyph = run.glyphs.iter().min_by_key(|g| g.start);
        let max_glyph = run.glyphs.iter().max_by_key(|g| g.end);

        for glyph in run.glyphs {
            if cursor.index >= glyph.start && cursor.index < glyph.end {
                let cluster = &run.text[glyph.start..glyph.end];
                let total = cluster.grapheme_indices(true).count().max(1);
                let before = cluster
                    .grapheme_indices(true)
                    .filter(|(off, _)| glyph.start + off < cursor.index)
                    .count();
                let frac = before as f32 / total as f32;
                let x = if glyph.level.is_rtl() {
                    glyph.x + glyph.w * (1.0 - frac)
                } else {
                    glyph.x + glyph.w * frac
                };
                return Some(Rectangle::new(
                    Point::new(x.clamp(0.0, (layout_width - 1.5).max(0.0)), run.line_top),
                    Size::new(1.5, run.line_height),
                ));
            }
        }

        if let Some(last_logical) = max_glyph
            && cursor.index >= last_logical.end
        {
            let x = if last_logical.level.is_rtl() {
                last_logical.x
            } else {
                last_logical.x + last_logical.w
            };
            let rect = Rectangle::new(
                Point::new(x.clamp(0.0, (layout_width - 1.5).max(0.0)), run.line_top),
                Size::new(1.5, run.line_height),
            );
            if cursor.index == last_logical.end {
                return Some(rect);
            }
            fallback_run = Some(rect);
        }

        if let Some(first_logical) = min_glyph
            && cursor.index <= first_logical.start
        {
            let x = if first_logical.level.is_rtl() {
                first_logical.x + first_logical.w
            } else {
                first_logical.x
            };
            fallback_run = Some(Rectangle::new(
                Point::new(x.clamp(0.0, (layout_width - 1.5).max(0.0)), run.line_top),
                Size::new(1.5, run.line_height),
            ));
        }
    }

    fallback_run
}

pub fn rtl_editor<'a, Message, Theme, Renderer>(
    content: &'a Content<Renderer>,
) -> RtlEditor<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: Catalog + 'a,
    Renderer: text::Renderer<Font = Font>,
{
    RtlEditor::new(content)
}

pub struct RtlEditor<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    id: Option<widget::Id>,
    content: &'a Content<Renderer>,
    placeholder: Option<Cow<'a, str>>,
    font: Option<Font>,
    text_size: Option<Pixels>,
    line_height: text::LineHeight,
    wrapping: Wrapping,
    width: Length,
    height: Length,
    min_height: f32,
    max_height: f32,
    padding: Padding,
    read_only: bool,
    on_edit: Option<Box<dyn Fn(Action) -> Message + 'a>>,
    class: Theme::Class<'a>,
}

impl<'a, Message, Theme, Renderer> RtlEditor<'a, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer<Font = Font>,
{
    pub fn new(content: &'a Content<Renderer>) -> Self {
        Self {
            id: None,
            content,
            placeholder: None,
            font: None,
            text_size: None,
            line_height: text::LineHeight::Relative(1.35),
            wrapping: Wrapping::Word,
            width: Length::Fill,
            height: Length::Shrink,
            min_height: 0.0,
            max_height: f32::INFINITY,
            padding: Padding::new(5.0),
            read_only: false,
            on_edit: None,
            class: <Theme as Catalog>::default(),
        }
    }

    pub fn id(mut self, id: impl Into<widget::Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<Cow<'a, str>>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn min_height(mut self, min_height: impl Into<Pixels>) -> Self {
        self.min_height = min_height.into().0;
        self
    }

    pub fn max_height(mut self, max_height: impl Into<Pixels>) -> Self {
        self.max_height = max_height.into().0;
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.text_size = Some(size.into());
        self
    }

    pub fn line_height(mut self, line_height: impl Into<text::LineHeight>) -> Self {
        self.line_height = line_height.into();
        self
    }

    pub fn font(mut self, font: impl Into<Font>) -> Self {
        self.font = Some(font.into());
        self
    }

    pub fn wrapping(mut self, wrapping: Wrapping) -> Self {
        self.wrapping = wrapping;
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn on_action(mut self, on_edit: impl Fn(Action) -> Message + 'a) -> Self {
        self.on_edit = Some(Box::new(on_edit));
        self
    }

    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self
    where
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as StyleFn<'a, Theme>).into();
        self
    }

    fn sync_layout(
        &self,
        internal: &mut Internal<Renderer>,
        width: f32,
        font: Font,
        text_size: Pixels,
    ) {
        let size_px = text_size.0.max(8.0);
        let line_h_px = self.line_height.to_absolute(Pixels(size_px)).0;
        let width_changed = (internal.layout_width - width).abs() > 0.25;
        let font_changed = internal.font != font;
        let size_changed = (internal.size - size_px).abs() > 0.1;
        let lh_changed = internal.line_height != self.line_height;
        let wrap_changed = internal.wrapping != self.wrapping;

        if internal.dirty_render
            || width_changed
            || font_changed
            || size_changed
            || lh_changed
            || wrap_changed
        {
            let mut fs = font_system().lock().expect("lock font system");
            let attrs = to_cosmic_attrs(font);
            let wrap = to_cosmic_wrap(self.wrapping);

            if internal.dirty_render {
                let text = internal.cached_text.clone();
                let cur = internal.cosmic_editor.cursor();
                let sel = internal.cosmic_editor.selection();
                internal.cosmic_editor.with_buffer_mut(|b| {
                    b.set_text(&mut fs, &text, &attrs, Shaping::Advanced, None);
                });
                internal.cosmic_editor.set_selection(sel);
                internal.cosmic_editor.set_cursor(cur);
            }

            internal.cosmic_editor.with_buffer_mut(|b| {
                if font_changed {
                    for line in b.lines.iter_mut() {
                        let _ = line.set_attrs_list(cosmic_text::AttrsList::new(&attrs));
                    }
                }
                b.set_metrics(&mut fs, Metrics::new(size_px, line_h_px));
                b.set_wrap(&mut fs, wrap);
                b.set_size(&mut fs, Some(width.max(1.0)), Some(1_000_000.0));
            });
            internal.cosmic_editor.shape_as_needed(&mut fs, false);
            internal.content_height = internal.cosmic_editor.with_buffer(measure_content_height);

            if internal.dirty_render {
                internal.render_editor = Renderer::Editor::with_text(&internal.cached_text);
            }
            internal.render_editor.update(
                Size::new(width.max(1.0), 1_000_000.0),
                font,
                Pixels(size_px),
                self.line_height,
                self.wrapping,
                &mut text::highlighter::PlainText,
            );

            internal.dirty_render = false;
            internal.layout_width = width;
            internal.font = font;
            internal.size = size_px;
            internal.line_height = self.line_height;
            internal.wrapping = self.wrapping;
        }
    }

    fn input_method(
        &self,
        state: &WidgetState,
        renderer: &Renderer,
        layout: Layout<'_>,
    ) -> InputMethod {
        if self.read_only || state.focus.is_none() {
            return InputMethod::Disabled;
        }
        let bounds = layout.bounds();
        let text_bounds = bounds.shrink(self.padding);
        let internal = self.content.0.borrow();
        let size_px = self.text_size.unwrap_or_else(|| renderer.default_size()).0;
        let caret = internal
            .cosmic_editor
            .with_buffer(|b| {
                compute_caret_rect(
                    b,
                    internal.cosmic_editor.cursor(),
                    detect_direction(&internal.cached_text, None).is_rtl(),
                )
            })
            .unwrap_or_else(|| Rectangle::new(Point::ORIGIN, Size::new(1.5, size_px * 1.35)));
        let screen_pos = Point::new(
            text_bounds.x + caret.x,
            (text_bounds.y + caret.y - internal.scroll_y).clamp(
                text_bounds.y,
                (text_bounds.y + text_bounds.height - caret.height).max(text_bounds.y),
            ),
        );

        InputMethod::Enabled {
            cursor: Rectangle::new(screen_pos, caret.size()),
            purpose: input_method::Purpose::Normal,
            preedit: state.preedit.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct Focus {
    updated_at: Instant,
    now: Instant,
    is_window_focused: bool,
}

impl Focus {
    const BLINK_MS: u128 = 500;

    fn now() -> Self {
        let now = Instant::now();
        Self {
            updated_at: now,
            now,
            is_window_focused: true,
        }
    }

    fn is_cursor_visible(&self) -> bool {
        self.is_window_focused
            && ((self.now - self.updated_at).as_millis() / Self::BLINK_MS).is_multiple_of(2)
    }
}

#[derive(Debug, Default)]
struct WidgetState {
    focus: Option<Focus>,
    preedit: Option<input_method::Preedit>,
    last_click: Option<mouse::Click>,
    drag_click: Option<mouse::click::Kind>,
    last_status: Option<Status>,
    modifiers: keyboard::Modifiers,
}

impl operation::Focusable for WidgetState {
    fn is_focused(&self) -> bool {
        self.focus.is_some()
    }

    fn focus(&mut self) {
        self.focus = Some(Focus::now());
    }

    fn unfocus(&mut self) {
        self.focus = None;
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for RtlEditor<'_, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer<Font = Font>,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<WidgetState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(WidgetState::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let limits = limits
            .width(self.width)
            .height(self.height)
            .min_height(self.min_height)
            .max_height(self.max_height);

        let inner_max = limits.shrink(self.padding).max();
        let font = self.font.unwrap_or_else(|| renderer.default_font());
        let text_size = self.text_size.unwrap_or_else(|| renderer.default_size());

        let mut internal = self.content.0.borrow_mut();
        self.sync_layout(&mut internal, inner_max.width, font, text_size);

        let node = match self.height {
            Length::Fill | Length::FillPortion(_) | Length::Fixed(_) => {
                layout::Node::new(limits.max())
            }
            Length::Shrink => {
                let desired_h = internal
                    .content_height
                    .clamp(self.min_height, self.max_height);
                layout::Node::new(limits.height(desired_h + self.padding.y()).max())
            }
        };

        let vp_h = (node.size().height - self.padding.y()).max(0.0);
        internal.viewport_height = vp_h;
        let max_scroll = (internal.content_height - vp_h).max(0.0);
        internal.scroll_y = internal.scroll_y.clamp(0.0, max_scroll);

        node
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<WidgetState>();
        let bounds = layout.bounds();
        let text_bounds = bounds.shrink(self.padding);
        let is_redraw = matches!(event, Event::Window(window::Event::RedrawRequested(_)));

        let publish = |action: Action, shell: &mut Shell<'_, Message>| {
            if let Some(on_edit) = self.on_edit.as_ref() {
                shell.publish(on_edit(action));
            }
        };

        match event {
            Event::Window(window::Event::Unfocused) => {
                if let Some(focus) = &mut state.focus {
                    focus.is_window_focused = false;
                }
            }
            Event::Window(window::Event::Focused) => {
                if let Some(focus) = &mut state.focus {
                    focus.is_window_focused = true;
                    focus.updated_at = Instant::now();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if let Some(focus) = &mut state.focus
                    && focus.is_window_focused
                    && !self.read_only
                {
                    focus.now = *now;
                    let millis_until_redraw =
                        Focus::BLINK_MS - (*now - focus.updated_at).as_millis() % Focus::BLINK_MS;
                    shell.request_redraw_at(
                        *now + std::time::Duration::from_millis(millis_until_redraw as u64),
                    );
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor.position_in(bounds) {
                    let click = mouse::Click::new(pos, mouse::Button::Left, state.last_click);
                    state.focus = Some(Focus::now());
                    state.last_click = Some(click);
                    state.drag_click = Some(click.kind());

                    let scroll_y = self.content.0.borrow().scroll_y;
                    let content_pos = Point::new(
                        (bounds.x + pos.x - text_bounds.x).clamp(0.0, text_bounds.width),
                        (bounds.y + pos.y - text_bounds.y + scroll_y).max(0.0),
                    );

                    let action = match click.kind() {
                        mouse::click::Kind::Single => Action::Click {
                            content_pos,
                            modifiers_shift: state.modifiers.shift(),
                        },
                        mouse::click::Kind::Double => Action::DoubleClick { content_pos },
                        mouse::click::Kind::Triple => Action::TripleClick { content_pos },
                    };
                    publish(action, shell);
                    shell.capture_event();
                    shell.request_redraw();
                } else if state.focus.is_some() {
                    state.focus = None;
                    state.drag_click = None;
                    shell.request_redraw();
                }
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
                state.modifiers = *m;
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.drag_click = None;
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.drag_click == Some(mouse::click::Kind::Single)
                    && let Some(pos) = cursor.position()
                {
                    let scroll_y = self.content.0.borrow().scroll_y;
                    let content_pos = Point::new(
                        (pos.x - text_bounds.x).clamp(0.0, text_bounds.width),
                        (pos.y - text_bounds.y + scroll_y).max(0.0),
                    );
                    publish(Action::Drag { content_pos }, shell);
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let (content_h, vp_h, scroll_y) = {
                    let internal = self.content.0.borrow();
                    (
                        internal.content_height,
                        internal.viewport_height,
                        internal.scroll_y,
                    )
                };
                let max_scroll = (content_h - vp_h).max(0.0);
                if max_scroll > 0.5 {
                    let dy = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => -y * 36.0,
                        mouse::ScrollDelta::Pixels { y, .. } => -y,
                    };
                    let at_top = scroll_y <= 0.1 && dy < 0.0;
                    let at_bottom = scroll_y >= max_scroll - 0.1 && dy > 0.0;
                    if !at_top && !at_bottom {
                        publish(Action::Scroll { delta_y: dy }, shell);
                        shell.capture_event();
                    }
                }
            }
            Event::InputMethod(ime_evt) if state.focus.is_some() && !self.read_only => {
                match ime_evt {
                    input_method::Event::Opened | input_method::Event::Closed => {
                        state.preedit = matches!(ime_evt, input_method::Event::Opened)
                            .then(input_method::Preedit::new);
                        shell.request_redraw();
                    }
                    input_method::Event::Preedit(content, selection) => {
                        state.preedit = Some(input_method::Preedit {
                            content: content.clone(),
                            selection: selection.clone(),
                            text_size: self.text_size,
                        });
                        shell.request_redraw();
                    }
                    input_method::Event::Commit(text) => {
                        publish(Action::Edit(Edit::Paste(Arc::new(text.clone()))), shell);
                    }
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                text,
                ..
            }) if state.focus.is_some() => {
                if let Some(focus) = &mut state.focus {
                    focus.updated_at = Instant::now();
                }
                let cmd = modifiers.command();
                let shift = modifiers.shift();

                let phys_code = match physical_key {
                    key::Physical::Code(c) => Some(*c),
                    _ => None,
                };
                let char_key = key.to_latin(*physical_key);

                if cmd && !modifiers.alt() {
                    if phys_code == Some(key::Code::KeyA) || char_key == Some('a') {
                        publish(Action::SelectAll, shell);
                        shell.capture_event();
                    } else if phys_code == Some(key::Code::KeyC) || char_key == Some('c') {
                        if let Some(sel) = self.content.selection() {
                            clipboard.write(clipboard::Kind::Standard, sel);
                        }
                        shell.capture_event();
                    } else if !self.read_only
                        && (phys_code == Some(key::Code::KeyX) || char_key == Some('x'))
                    {
                        if let Some(sel) = self.content.selection() {
                            clipboard.write(clipboard::Kind::Standard, sel);
                            publish(Action::Edit(Edit::Cut), shell);
                        }
                        shell.capture_event();
                    } else if !self.read_only
                        && (phys_code == Some(key::Code::KeyV) || char_key == Some('v'))
                    {
                        if let Some(clip) = clipboard.read(clipboard::Kind::Standard) {
                            publish(Action::Edit(Edit::Paste(Arc::new(clip))), shell);
                        }
                        shell.capture_event();
                    } else {
                        let motion = match key.as_ref() {
                            keyboard::Key::Named(key::Named::ArrowLeft) => Some(Motion::WordLeft),
                            keyboard::Key::Named(key::Named::ArrowRight) => Some(Motion::WordRight),
                            keyboard::Key::Named(key::Named::Home) => Some(Motion::DocumentStart),
                            keyboard::Key::Named(key::Named::End) => Some(Motion::DocumentEnd),
                            _ => None,
                        };
                        if let Some(m) = motion {
                            publish(
                                Action::Move {
                                    motion: m,
                                    select: shift,
                                },
                                shell,
                            );
                            shell.capture_event();
                        }
                    }
                } else {
                    match key.as_ref() {
                        keyboard::Key::Named(key::Named::ArrowLeft) => {
                            publish(
                                Action::Move {
                                    motion: Motion::Left,
                                    select: shift,
                                },
                                shell,
                            );
                            shell.capture_event();
                        }
                        keyboard::Key::Named(key::Named::ArrowRight) => {
                            publish(
                                Action::Move {
                                    motion: Motion::Right,
                                    select: shift,
                                },
                                shell,
                            );
                            shell.capture_event();
                        }
                        keyboard::Key::Named(key::Named::ArrowUp) => {
                            publish(
                                Action::Move {
                                    motion: Motion::Up,
                                    select: shift,
                                },
                                shell,
                            );
                            shell.capture_event();
                        }
                        keyboard::Key::Named(key::Named::ArrowDown) => {
                            publish(
                                Action::Move {
                                    motion: Motion::Down,
                                    select: shift,
                                },
                                shell,
                            );
                            shell.capture_event();
                        }
                        keyboard::Key::Named(key::Named::Home) => {
                            publish(
                                Action::Move {
                                    motion: Motion::Home,
                                    select: shift,
                                },
                                shell,
                            );
                            shell.capture_event();
                        }
                        keyboard::Key::Named(key::Named::End) => {
                            publish(
                                Action::Move {
                                    motion: Motion::End,
                                    select: shift,
                                },
                                shell,
                            );
                            shell.capture_event();
                        }
                        keyboard::Key::Named(key::Named::PageUp) => {
                            publish(
                                Action::Move {
                                    motion: Motion::PageUp,
                                    select: shift,
                                },
                                shell,
                            );
                            shell.capture_event();
                        }
                        keyboard::Key::Named(key::Named::PageDown) => {
                            publish(
                                Action::Move {
                                    motion: Motion::PageDown,
                                    select: shift,
                                },
                                shell,
                            );
                            shell.capture_event();
                        }
                        keyboard::Key::Named(key::Named::Enter) if !self.read_only => {
                            publish(Action::Edit(Edit::Enter), shell);
                            shell.capture_event();
                        }
                        keyboard::Key::Named(key::Named::Backspace) if !self.read_only => {
                            publish(Action::Edit(Edit::Backspace), shell);
                            shell.capture_event();
                        }
                        keyboard::Key::Named(key::Named::Delete) if !self.read_only => {
                            publish(Action::Edit(Edit::Delete), shell);
                            shell.capture_event();
                        }
                        keyboard::Key::Named(key::Named::Space) if !self.read_only => {
                            publish(Action::Edit(Edit::Insert(' ')), shell);
                            shell.capture_event();
                        }
                        keyboard::Key::Named(key::Named::Escape) => {
                            state.focus = None;
                            shell.capture_event();
                            shell.request_redraw();
                        }
                        _ if !self.read_only && !modifiers.alt() => {
                            if let Some(txt) = text.as_ref() {
                                let mut inserted = false;
                                for ch in txt.chars() {
                                    if !ch.is_control() {
                                        publish(Action::Edit(Edit::Insert(ch)), shell);
                                        inserted = true;
                                    }
                                }
                                if inserted {
                                    shell.capture_event();
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }

        let is_hovered = cursor.is_over(bounds);
        let status = if self.on_edit.is_none() {
            Status::Disabled
        } else if state.focus.is_some() {
            Status::Focused { is_hovered }
        } else if is_hovered {
            Status::Hovered
        } else {
            Status::Active
        };

        if is_redraw {
            state.last_status = Some(status);
            shell.request_input_method(&self.input_method(state, renderer, layout));
        } else if state.last_status.is_some_and(|prev| prev != status) {
            state.last_status = Some(status);
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _defaults: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<WidgetState>();
        let is_hovered = cursor.is_over(bounds);
        let status = if self.on_edit.is_none() {
            Status::Disabled
        } else if state.focus.is_some() {
            Status::Focused { is_hovered }
        } else if is_hovered {
            Status::Hovered
        } else {
            Status::Active
        };

        let style = theme.style(&self.class, status);
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: style.border,
                ..renderer::Quad::default()
            },
            style.background,
        );

        let text_bounds = bounds.shrink(self.padding);
        let font = self.font.unwrap_or_else(|| renderer.default_font());
        let text_size = self.text_size.unwrap_or_else(|| renderer.default_size());

        {
            let mut internal = self.content.0.borrow_mut();
            self.sync_layout(&mut internal, text_bounds.width, font, text_size);
        }

        let internal = self.content.0.borrow();
        let scroll_y = internal.scroll_y;
        let offset = Vector::new(text_bounds.x, text_bounds.y - scroll_y);

        // Draw RTL/BiDi-accurate selection rectangles first (underneath text glyphs).
        if let Some((sel_start, sel_end)) = internal.cosmic_editor.selection_bounds() {
            let rects = internal
                .cosmic_editor
                .with_buffer(|b| compute_selection_rects(b, sel_start, sel_end));
            for rect in rects {
                let translated = Rectangle::new(rect.position() + offset, rect.size());
                if let Some(clipped) = translated.intersection(&text_bounds) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: clipped,
                            ..renderer::Quad::default()
                        },
                        style.selection,
                    );
                }
            }
        }

        if internal.cached_text.is_empty() {
            if let Some(placeholder) = self.placeholder.as_ref() {
                renderer.fill_text(
                    text::Text {
                        content: placeholder.as_ref().to_string(),
                        bounds: text_bounds.size(),
                        size: text_size,
                        line_height: self.line_height,
                        font,
                        align_x: text::Alignment::Default,
                        align_y: iced::alignment::Vertical::Top,
                        shaping: text::Shaping::Advanced,
                        wrapping: self.wrapping,
                    },
                    text_bounds.position(),
                    style.placeholder,
                    text_bounds,
                );
            }
        } else {
            renderer.fill_editor(
                &internal.render_editor,
                Point::new(text_bounds.x, text_bounds.y - scroll_y),
                style.value,
                text_bounds,
            );
        }

        // Draw caret when focused and editable (without active range selection).
        if !self.read_only
            && internal.cosmic_editor.selection_bounds().is_none()
            && let Some(focus) = state.focus.as_ref()
            && focus.is_cursor_visible()
        {
            let default_rtl = detect_direction(&internal.cached_text, None).is_rtl();
            if let Some(caret_local) = internal.cosmic_editor.with_buffer(|b| {
                compute_caret_rect(b, internal.cosmic_editor.cursor(), default_rtl)
            }) {
                let translated =
                    Rectangle::new(caret_local.position() + offset, caret_local.size());
                if let Some(clipped) = translated.intersection(&text_bounds) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: clipped,
                            ..renderer::Quad::default()
                        },
                        style.value,
                    );
                }
            }
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        let state = tree.state.downcast_mut::<WidgetState>();
        operation.focusable(self.id.as_ref(), layout.bounds(), state);
    }
}

impl<'a, Message, Theme, Renderer> From<RtlEditor<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: Catalog + 'a,
    Renderer: text::Renderer<Font = Font> + 'a,
{
    fn from(editor: RtlEditor<'a, Message, Theme, Renderer>) -> Self {
        Self::new(editor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaped_buffer(text: &str, width: f32) -> Buffer {
        let mut fs = font_system().lock().expect("lock font system");
        let mut buffer = Buffer::new_empty(Metrics::new(14.0, 20.0));
        buffer.set_text(&mut fs, text, &Attrs::new(), Shaping::Advanced, None);
        buffer.set_wrap(&mut fs, Wrap::Word);
        buffer.set_size(&mut fs, Some(width), Some(1_000.0));
        buffer.shape_until_scroll(&mut fs, false);
        buffer
    }

    #[test]
    fn rtl_selection_highlights_on_right_side_not_left_edge() {
        let text = "سلام دنیا";
        let buf = shaped_buffer(text, 400.0);
        let start = CosmicCursor::new(0, 0);
        let end = CosmicCursor::new(0, text.len());
        let rects = compute_selection_rects(&buf, start, end);
        assert_eq!(rects.len(), 1);
        let rect = rects[0];
        // Text is only ~50px wide in a 400px box; RTL alignment puts it near x=350..400, NOT x=0!
        assert!(
            rect.x > 250.0,
            "expected RTL selection rect on right side (>250), got {:?}",
            rect
        );
        assert!(rect.x + rect.width <= 401.0);
    }

    #[test]
    fn rtl_partial_selection_matches_selected_word_glyphs() {
        let text = "سلام خوبی";
        let buf = shaped_buffer(text, 400.0);
        let full = compute_selection_rects(
            &buf,
            CosmicCursor::new(0, 0),
            CosmicCursor::new(0, text.len()),
        )[0];
        let first_word_end = text.find(' ').unwrap();
        let first_word = compute_selection_rects(
            &buf,
            CosmicCursor::new(0, 0),
            CosmicCursor::new(0, first_word_end),
        )[0];

        // First logical word in RTL ("سلام") must align against the RIGHT edge of the full line!
        let diff_right = ((full.x + full.width) - (first_word.x + first_word.width)).abs();
        assert!(
            diff_right < 1.5,
            "first RTL word right edge should align with line right edge: full={:?}, first={:?}",
            full,
            first_word
        );
    }

    #[test]
    fn rtl_hit_snaps_right_empty_space_to_start_and_left_empty_space_to_end() {
        let text = "سلام دنیا";
        let buf = shaped_buffer(text, 400.0);
        let hit_right_edge = rtl_aware_hit(&buf, 398.0, 10.0).expect("hit right");
        assert_eq!(hit_right_edge.index, 0);

        let hit_left_empty = rtl_aware_hit(&buf, 20.0, 10.0).expect("hit left empty space");
        assert_eq!(hit_left_empty.index, text.len());
    }

    #[test]
    fn rtl_drag_select_in_content_selects_expected_substring() {
        let mut content: Content<iced::Renderer> = Content::with_text("سلام دنیا");
        // Perform layout pass to wrap at 400px
        {
            let mut internal = content.0.borrow_mut();
            let mut fs = font_system().lock().unwrap();
            internal
                .cosmic_editor
                .with_buffer_mut(|b| b.set_size(&mut fs, Some(400.0), Some(1_000.0)));
            internal.cosmic_editor.shape_as_needed(&mut fs, false);
        }

        // Click at right edge (start of RTL line), drag to x=20 (past left edge of text)
        content.perform(Action::Click {
            content_pos: Point::new(399.0, 8.0),
            modifiers_shift: false,
        });
        content.perform(Action::Drag {
            content_pos: Point::new(20.0, 8.0),
        });
        assert_eq!(content.selection().as_deref(), Some("سلام دنیا"));
    }

    #[test]
    fn ltr_and_caret_positions_stay_coherent() {
        let text = "Hello world";
        let buf = shaped_buffer(text, 400.0);
        let caret_start = compute_caret_rect(&buf, CosmicCursor::new(0, 0), false).unwrap();
        let caret_end = compute_caret_rect(&buf, CosmicCursor::new(0, text.len()), false).unwrap();
        assert!(caret_start.x < 1.0);
        assert!(caret_end.x > caret_start.x + 20.0);

        let rects = compute_selection_rects(&buf, CosmicCursor::new(0, 0), CosmicCursor::new(0, 5));
        assert_eq!(rects.len(), 1);
        assert!(rects[0].x < 1.0 && rects[0].width > 15.0);
    }
}
