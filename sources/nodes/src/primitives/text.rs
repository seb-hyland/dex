use dex_core::prelude::*;
use egui::{
    Align, Color32, FontId, Frame, Layout, Margin, Pos2, Rect, TextEdit, UiBuilder,
    text::{LayoutJob, TextWrapping},
};
use egui_code_editor::{CodeEditor as CodeEditorWidget, ColorTheme, DEFAULT_THEMES, Syntax};
use utils::Transient;

use crate::layouts::vertical::VerticalLayout;
use crate::primitives::checkbox::{Checkbox, TakeToggled};
use crate::primitives::color_picker::{ColorPicker, TakePickedColor};

#[utils::dynamic_type]
#[utils::portable]
pub struct Label {
    pub text: String,
    pub singleline: bool,

    pub font: Font,
    pub color: Color,
}

#[utils::dynamic_node]
impl Node for Label {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Label".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;

        // A continuation is a char offset
        let start = if let WrapConstraints::CanRequest { continuation, .. } = constraints.wrap
            && let Some(cont) = continuation
        {
            cont as usize
        } else {
            0
        };
        let remaining: String = self.text.chars().skip(start).collect();
        if remaining.is_empty() {
            // Nothing left to render
            return DrawResult::Complete { region: None };
        }

        let avail_w = constraints.x.map(|a| a.provided_value());
        let avail_h = constraints.y.map(|a| a.provided_value());

        if self.singleline {
            self.draw_singleline(&mut ctx, &constraints, &remaining, avail_w)
        } else {
            self.draw_multiline(&mut ctx, &constraints, &remaining, start, avail_w, avail_h)
        }
    }

    fn build_inspector(&self, ctx: NodeContext) -> Option<NodeUid> {
        Some(
            TextStyleInspector::build(
                ctx,
                ctx.id,
                TextStyle::of(self.font, self.singleline, self.color),
            )
            .erase(),
        )
    }
}

defhandlers! { Label {
    actions: [
        SetBold { on: bool } => (this, s) { this.font.bold = s.on },
        SetItalic { on: bool } => (this, s) { this.font.italic = s.on },
        SetUnderline { on: bool } => (this, s) { this.font.underline = s.on },
        SetSingleline { on: bool } => (this, s) { this.singleline = s.on },
        SetTextColor { color: Color } => (this, s) { this.color = s.color },
    ],
}}

#[utils::dynamic_methods]
impl Label {
    pub fn new(text: String) -> Self {
        Self {
            text,
            singleline: true,
            font: Font::proportional(16.0),
            color: Color::BLACK,
        }
    }

    fn draw_singleline(
        &self,
        ctx: &mut DrawContext,
        constraints: &DrawConstraints,
        remaining: &str,
        avail_w: Option<f32>,
    ) -> DrawResult {
        let mut job = LayoutJob::single_section(
            remaining.to_owned(),
            self.font.text_format(ctx.ui.ctx(), self.color),
        );
        job.break_on_newline = false;
        let galley = ctx.ui.ctx().fonts_mut(|f| f.layout_job(job.clone()));

        let fits = avail_w.is_none_or(|w| galley.rect.width() <= w);
        if fits {
            // We can render here normally!
            let origin: Pos2 = constraints.pos.into();
            ctx.ui
                .painter()
                .galley(origin, galley.clone(), self.color.into());
            return DrawResult::Complete {
                region: Some(galley.rect.translate(origin.to_vec2()).into()),
            };
        }

        if constraints.wrap.can_retry_on_newline() {
            // Request to draw on a new line
            return DrawResult::Wrap {
                region: None,
                continuation: 0,
            };
        }

        let w = avail_w.unwrap_or(f32::INFINITY);
        // Update job to truncate at max size
        job.wrap = TextWrapping::truncate_at_width(w);
        let galley = ctx.ui.ctx().fonts_mut(|f| f.layout_job(job));

        let origin: Pos2 = constraints.pos.into();
        ctx.ui
            .painter()
            .galley(origin, galley.clone(), self.color.into());
        DrawResult::Complete {
            region: Some(galley.rect.translate(origin.to_vec2()).into()),
        }
    }

    fn draw_multiline(
        &self,
        ctx: &mut DrawContext,
        constraints: &DrawConstraints,
        remaining: &str,
        start: usize,
        avail_w: Option<f32>,
        avail_h: Option<f32>,
    ) -> DrawResult {
        let width = avail_w.unwrap_or(f32::INFINITY);
        let height = avail_h.unwrap_or(f32::INFINITY);
        let wrap_constraints = constraints.wrap;

        let mut job = LayoutJob::single_section(
            remaining.to_owned(),
            self.font.text_format(ctx.ui.ctx(), self.color),
        );
        job.wrap.max_width = width;
        let galley = ctx.ui.ctx().fonts_mut(|f| f.layout_job(job));

        let first_row_height = galley.rows[0].height();
        if first_row_height > height {
            // There is not enough height to draw

            if matches!(wrap_constraints, WrapConstraints::NotAllowed)
                || !wrap_constraints.can_retry_on_newline()
            {
                // No space to draw any more rows
                return DrawResult::Complete { region: None };
            }

            // Start the drawing process on a new line
            return DrawResult::Wrap {
                region: None,
                continuation: 0,
            };
        }

        let num_rows_to_layout = if matches!(wrap_constraints, WrapConstraints::CanRequest { .. }) {
            // Draw one row and then request wrap
            1
        } else {
            // Draw as much as possible in a vertical chunk of text
            galley
                .rows
                .iter()
                .take_while(|pr| pr.rect().max.y <= height)
                .count()
        };

        // The local bounding rect of all rows that should be displayed
        let local_rect = galley.rows[..num_rows_to_layout]
            .iter()
            .fold(Rect::NOTHING, |acc, pr| acc.union(pr.rect()));
        let origin: Pos2 = constraints.pos.into();

        let mut clip_rect = ctx.ui.clip_rect();
        clip_rect.max.y = clip_rect.max.y.min(origin.y + local_rect.max.y);
        ctx.ui.painter().with_clip_rect(clip_rect).galley(
            origin,
            galley.clone(),
            self.color.into(),
        );

        let draw_region = local_rect.translate(origin.to_vec2()).into();
        let have_rows_remaining = galley.rows.len() > num_rows_to_layout;

        if matches!(wrap_constraints, WrapConstraints::CanRequest { .. }) && have_rows_remaining {
            let chars_consumed: usize = galley
                .rows
                .iter()
                .take(num_rows_to_layout)
                .map(|pr| pr.char_count_including_newline().0)
                .sum();
            // Continue drawing on the next line
            DrawResult::Wrap {
                region: Some(draw_region),
                continuation: (start + chars_consumed) as u64,
            }
        } else {
            // Done drawing
            DrawResult::Complete {
                region: Some(draw_region),
            }
        }
    }
}

#[utils::dynamic_type]
#[utils::portable]
pub struct LabelEditable {
    pub value: String,
    buf: Transient<String>,

    pub singleline: bool,
    pub shrink_to_text: bool,

    pub interactive: bool,
    /// Grab focus when this node becomes interactive and lock back to non-interactive on focus loss.
    pub auto_lock: bool,

    pub font: Font,
    pub color: Color,
}

#[utils::dynamic_methods]
impl LabelEditable {
    pub fn new(value: String) -> Self {
        Self {
            value,
            buf: Transient::default(),
            singleline: true,
            shrink_to_text: true,
            interactive: true,
            auto_lock: false,
            font: Font::proportional(16.0),
            color: Color::BLACK,
        }
    }

    /// The current text.
    #[dynamic(skip)]
    pub fn resolved_text(&self) -> String {
        self.buf.val().clone().unwrap_or_else(|| self.value.clone())
    }

    /// A label that starts as static text, becomes editable on [`SetInteractive`], and locks again on focus loss.
    pub fn click_to_edit(value: String) -> Self {
        Self {
            interactive: false,
            auto_lock: true,
            ..Self::new(value)
        }
    }

    /// Render the committed value as static, non-interactive text.
    fn draw_static(&self, ctx: DrawContext) -> DrawResult {
        let mut job = LayoutJob::single_section(
            self.value.clone(),
            self.font.text_format(ctx.ui.ctx(), self.color),
        );
        job.break_on_newline = false;
        let galley = ctx.ui.ctx().fonts_mut(|f| f.layout_job(job));
        let content_w = galley.rect.width();
        let row_h = galley.rows[0].height();

        let avail_w = ctx.constraints.x.map(|a| a.provided_value());
        let exact_w = match ctx.constraints.x {
            Some(AxisConstraint::Exactly(w)) => Some(w),
            _ => None,
        };
        let exact_h = match ctx.constraints.y {
            Some(AxisConstraint::Exactly(h)) => Some(h),
            _ => None,
        };

        const CARET_PADDING: f32 = 2.0;
        let min_w = row_h;
        let mut block_w = exact_w.unwrap_or_else(|| {
            if self.shrink_to_text {
                (content_w + CARET_PADDING).max(min_w)
            } else {
                avail_w.unwrap_or((content_w + CARET_PADDING).max(min_w))
            }
        });

        if let Some(w) = avail_w
            && block_w > w
        {
            if ctx.constraints.wrap.can_retry_on_newline() {
                return DrawResult::Wrap {
                    region: None,
                    continuation: 0,
                };
            } else {
                block_w = w;
            }
        }

        let size = Vector {
            x: block_w,
            y: exact_h.unwrap_or(row_h),
        };
        let origin = ctx.constraints.pos;
        // Mirror the editor's centered alignment.
        let text_pos = Pos2 {
            x: origin.x + ((block_w - content_w) * 0.5).max(0.0),
            y: origin.y + (size.y - row_h) * 0.5,
        };
        ctx.ui.painter().galley(text_pos, galley, self.color.into());

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, size)),
        }
    }
}

#[utils::dynamic_node]
impl Node for LabelEditable {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "An Editable Label".to_owned()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        if !self.interactive {
            return self.draw_static(ctx);
        }

        let avail_w = ctx.constraints.x.map(|a| a.provided_value());
        let avail_h = ctx.constraints.y.map(|a| a.provided_value());

        // Measure the width of the text field
        let content = self.buf.val_or_else(|| self.value.clone()).clone();
        let mut measure_job =
            LayoutJob::single_section(content, self.font.text_format(ctx.ui.ctx(), self.color));
        measure_job.break_on_newline = false;
        let galley = ctx.ui.ctx().fonts_mut(|f| f.layout_job(measure_job));
        let content_w = galley.rect.width();
        let row_h = galley.rows[0].height();

        // When the parent demands an exact size fill it; otherwise size to the content.
        let exact_w = match ctx.constraints.x {
            Some(AxisConstraint::Exactly(w)) => Some(w),
            _ => None,
        };
        let exact_h = match ctx.constraints.y {
            Some(AxisConstraint::Exactly(h)) => Some(h),
            _ => None,
        };

        // Horizontal fit --------------------------------------------------
        let min_w = row_h; // if field is empty, make a square the height of one row
        const CARET_PADDING: f32 = 2.0; // padding size so cursor remains visible
        let mut block_w = exact_w.unwrap_or_else(|| {
            if self.shrink_to_text {
                (content_w + CARET_PADDING).max(min_w)
            } else {
                avail_w.unwrap_or((content_w + CARET_PADDING).max(min_w))
            }
        });

        if let Some(w) = avail_w
            && block_w > w
        {
            if ctx.constraints.wrap.can_retry_on_newline() {
                return DrawResult::Wrap {
                    region: None,
                    continuation: 0,
                };
            } else {
                block_w = w;
            }
        }

        // Vertical fit --------------------------------------------------
        if let Some(h) = avail_h
            && row_h > h
            &&
            // Draw anyways if no retry possibility
            ctx.constraints.wrap.can_retry_on_newline()
        {
            return DrawResult::Wrap {
                region: None,
                continuation: 0,
            };
        }

        let size = Vector {
            x: block_w,
            y: exact_h.unwrap_or(row_h),
        };
        let origin = ctx.constraints.pos;

        let rect = Rect::from_min_size(origin.into(), size.into());

        let mut buf_mut = self.buf.val_mut_or_else(|| self.value.clone());
        let editor_id = egui::Id::new(ctx.node.id);

        let format = self.font.text_format(ctx.ui.ctx(), self.color);
        let multiline = !self.singleline;
        let mut layouter = move |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut job = LayoutJob::single_section(text.as_str().to_owned(), format.clone());
            job.break_on_newline = multiline;
            job.wrap.max_width = if multiline { wrap_width } else { f32::INFINITY };
            job.halign = Align::Center;
            // Hiding trailing whitespace feels wrong while typing.
            job.keep_trailing_whitespace = true;
            ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
        };
        let editor = if self.singleline {
            TextEdit::singleline(&mut *buf_mut)
        } else {
            TextEdit::multiline(&mut *buf_mut)
        }
        .id(editor_id)
        .frame(Frame::NONE)
        .margin(Margin::ZERO)
        .font(self.font.font_id_in(ctx.ui.ctx()))
        .text_color(self.color.into())
        .layouter(&mut layouter)
        .horizontal_align(Align::Center)
        .vertical_align(Align::Center)
        .desired_width(block_w);

        if self.auto_lock {
            // A freshly-activated click-to-edit label grabs focus so typing works immediately.
            let known = ctx.ui.memory(|mem| {
                mem.has_focus(editor_id) ||
                    // Do not want to re-focus if focus was just dropped
                    mem.had_focus_last_frame(editor_id)
            });
            if !known {
                ctx.ui.memory_mut(|mem| mem.request_focus(editor_id));
            }
        }

        // Insert the widget flush inside the computed block.
        let editor_response = ctx
            .ui
            .scope_builder(
                UiBuilder::new()
                    .max_rect(rect)
                    .id_salt(ctx.node.id)
                    .layout(Layout::left_to_right(Align::Min)),
                |ui| ui.add(editor),
            )
            .inner;

        if editor_response.lost_focus() {
            ctx.submit_action_for_self::<Self, _>(
                SetText {
                    value: buf_mut.clone(),
                },
                "Updated editable label's stored value on focus loss",
            );
            if self.auto_lock {
                ctx.submit_action_for_self::<Self, _>(
                    SetInteractive { on: false },
                    "Locked click-to-edit label on focus loss",
                );
            }
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, size)),
        }
    }

    fn build_inspector(&self, ctx: NodeContext) -> Option<NodeUid> {
        Some(
            TextStyleInspector::build(
                ctx,
                ctx.id,
                TextStyle::of(self.font, self.singleline, self.color),
            )
            .erase(),
        )
    }
}

defhandlers! { LabelEditable {
    actions: [
        // Update the live edit buffer too, so a programmatic `SetText` is
        // reflected in the interactive display (not just the committed `value`,
        // which the editing buffer would otherwise shadow).
        SetText { value: String } => (this, s) {
            this.buf.set(s.value.clone());
            this.value = s.value;
        },
        SetInteractive { on: bool } => (this, s) { this.interactive = s.on },
    ],
    extern_actions: [
        // The styling an inspector offers is the plain label's.
        SetBold => (this, s) { this.font.bold = s.on },
        SetItalic => (this, s) { this.font.italic = s.on },
        SetUnderline => (this, s) { this.font.underline = s.on },
        SetSingleline => (this, s) { this.singleline = s.on },
        SetTextColor => (this, s) { this.color = s.color },
    ],
    requests: [
        IsInteractive => (this, _q): bool { this.interactive },
    ],
    extern_requests: [
        GetText => (this, _q): String { this.resolved_text() },
    ],
}}

/// A label's styling, as an inspector needs it to start from.
#[derive(Clone, Copy)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub singleline: bool,
    pub color: Color,
}

impl TextStyle {
    fn of(font: Font, singleline: bool, color: Color) -> Self {
        Self {
            bold: font.bold,
            italic: font.italic,
            underline: font.underline,
            singleline,
            color,
        }
    }
}

/// Styling controls for a [`Label`] or a [`LabelEditable`].
#[utils::portable]
pub struct TextStyleInspector {
    /// The label these controls style. Both label kinds take them.
    #[uid_ref]
    target: NodeUid,
    bold: NodeUid<Checkbox>,
    italic: NodeUid<Checkbox>,
    underline: NodeUid<Checkbox>,
    singleline: NodeUid<Checkbox>,
    color: NodeUid<ColorPicker>,
    column: NodeUid<VerticalLayout>,
}

impl TextStyleInspector {
    fn build(ctx: NodeContext, target: NodeUid, style: TextStyle) -> NodeUid<TextStyleInspector> {
        let ws = ctx.workspace.action_handle();
        let tick = |label: &str, on: bool| Checkbox::build(ws.clone(), label.to_owned(), on);

        let bold = tick("Bold", style.bold);
        let italic = tick("Italic", style.italic);
        let underline = tick("Underline", style.underline);
        let singleline = tick("Single line", style.singleline);
        let color = ColorPicker::build(ws.clone(), "Colour".to_owned(), style.color);

        let column = VerticalLayout::build(
            ws.clone(),
            vec![
                bold.erase(),
                italic.erase(),
                underline.erase(),
                singleline.erase(),
                color.erase(),
            ],
            3.0,
        );
        ws.insert_node(Self {
            target,
            bold,
            italic,
            underline,
            singleline,
            color,
            column,
        })
    }
}

#[utils::dynamic_node(skip)]
impl Node for TextStyleInspector {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "Text Styling".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        let drawn = ctx.draw_workspace_node(self.column.erase(), constraints);

        let ws = ctx.node.workspace;
        let target = self.target;
        let toggled = |tick: NodeUid<Checkbox>| ws.send_request(tick, TakeToggled).flatten();

        if let Some(on) = toggled(self.bold) {
            ws.submit_action(target, "Set the label's weight", SetBold { on });
        }
        if let Some(on) = toggled(self.italic) {
            ws.submit_action(target, "Set the label's slant", SetItalic { on });
        }
        if let Some(on) = toggled(self.underline) {
            ws.submit_action(target, "Underlined the label", SetUnderline { on });
        }
        if let Some(on) = toggled(self.singleline) {
            ws.submit_action(target, "Set the label's wrapping", SetSingleline { on });
        }
        if let Some(color) = ws.send_request(self.color, TakePickedColor).flatten() {
            ws.submit_action(target, "Recoloured the label", SetTextColor { color });
        }

        drawn.unwrap_or(DrawResult::Complete { region: None })
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.column.erase());
        for tick in [self.bold, self.italic, self.underline, self.singleline] {
            ctx.workspace.delete_node(tick.erase());
        }
        ctx.workspace.delete_node(self.color.erase());
    }
}

defhandlers! { TextStyleInspector {} }

#[utils::dynamic_type]
#[utils::portable]
pub struct CodeEditor {
    pub value: String,
    buf: Transient<String>,

    pub font_size: f32,
    pub rows: usize,
    pub numlines: bool,

    /// When set, grow to fill the available height.
    pub fill: bool,

    /// Colour theme, matched by name against [`DEFAULT_THEMES`] (e.g. "Gruvbox")
    pub theme: String,
    /// Highlighting language (e.g. "rust", "python")
    pub language: String,
}

#[utils::dynamic_methods]
impl CodeEditor {
    pub fn new(value: String, language: String) -> Self {
        Self {
            value,
            buf: Transient::default(),
            font_size: 14.0,
            rows: 6,
            numlines: true,
            fill: false,
            theme: "Github Light".to_owned(),
            language,
        }
    }
}

#[utils::dynamic_node]
impl Node for CodeEditor {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Code Editor".to_owned()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let avail_w = ctx.constraints.x.map(|a| a.provided_value());
        let avail_h = ctx.constraints.y.map(|a| a.provided_value());

        // Since a monospace font is used, we can reason about fit by determining the width of a single character
        let probe = LayoutJob::simple_singleline(
            "0".to_owned(),
            FontId::monospace(self.font_size),
            Color32::WHITE,
        );
        let galley = ctx.ui.ctx().fonts_mut(|f| f.layout_job(probe));
        let row_h = galley.rows[0].height();
        let char_w = galley.rect.width();

        // Vertical fit --------------------------------------------------
        if let Some(h) = avail_h
            && row_h > h
            && ctx.constraints.wrap.can_retry_on_newline()
        {
            return DrawResult::Wrap {
                region: None,
                continuation: 0,
            };
        }

        // Horizontal fit --------------------------------------------------
        const MIN_COLS: f32 = 8.0;
        let min_w = char_w * MIN_COLS;
        if let Some(w) = avail_w
            && w < min_w
            // Draw anyways if no retry possibility
            && ctx.constraints.wrap.can_retry_on_newline()
        {
            return DrawResult::Wrap {
                region: None,
                continuation: 0,
            };
        }

        // Determine editor size and region
        const FALLBACK_WIDTH: f32 = 400.0;
        // Frame padding plus the text edit's own vertical margin.
        const CHROME_H: f32 = 8.0;
        let block_w = avail_w.unwrap_or(FALLBACK_WIDTH);

        // The editor is `rows` tall and lets its internal scroll area handle anything longer.
        let natural_h = row_h * self.rows.max(1) as f32 + CHROME_H;
        let block_h = match avail_h {
            Some(h) if self.fill => h,
            Some(h) => h.min(natural_h),
            None => natural_h,
        };
        let size = Vector {
            x: block_w,
            y: block_h,
        };
        let origin = ctx.constraints.pos;
        let rect = Rect::from_min_size(origin.into(), size.into());

        // When filling, derive the visible row count from the available height.
        let rows = if self.fill && row_h > 0.0 {
            (block_h / row_h).floor().max(1.0) as usize
        } else {
            self.rows
        };

        let syntax = syntax_for(&self.language);
        let editor_id = egui::Id::new(ctx.node.id);
        let mut editor = CodeEditorWidget::default()
            .with_id(editor_id)
            .with_fontsize(self.font_size)
            .with_rows(rows)
            .with_numlines(self.numlines)
            .with_theme(theme_for(&self.theme))
            .desired_width(block_w);

        // Update on focus loss
        if ctx
            .ui
            .memory_mut(|mem| mem.had_focus_last_frame(editor_id) && !mem.has_focus(editor_id))
            && let Some(v) = &*self.buf.val()
        {
            ctx.submit_action_for_self::<Self, _>(
                SetText { value: v.clone() },
                "Updated editable label's stored value on focus loss",
            );
        }

        let mut buf_mut = self.buf.val_mut_or_else(|| self.value.clone());
        let drawn = ctx.ui.scope_builder(
            UiBuilder::new()
                .max_rect(rect)
                .id_salt(ctx.node.id)
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                editor.show(ui, &mut *buf_mut, &syntax);
            },
        );

        let drawn_region: ScreenRegion = rect.intersect(drawn.response.rect).into();
        DrawResult::Complete {
            region: Some(drawn_region),
        }
    }
}

defhandlers! { CodeEditor {
    extern_actions: [
        SetText => (this, s) { this.value = s.value },
    ],
    requests: [
        // The live buffer if mid-edit, else the committed value — `value` only
        // catches up on focus loss.
        GetText => (this, _q): String {
            match this.buf.val().as_ref() {
                Some(text) => text.clone(),
                None => this.value.clone(),
            }
        },
        GetCommittedText => (this, _q): String {
            this.value.clone()
        },
    ],
}}

/// Resolve a theme name against the bundled themes, defaulting to Gruvbox.
fn theme_for(name: &str) -> ColorTheme {
    DEFAULT_THEMES
        .iter()
        .copied()
        .find(|t| t.name().eq_ignore_ascii_case(name))
        .unwrap_or(ColorTheme::GRUVBOX)
}

/// Resolve a language name to a highlighting syntax, defaulting to plain text.
fn syntax_for(language: &str) -> Syntax {
    match language.to_ascii_lowercase().as_str() {
        "rust" | "rs" => Syntax::rust(),
        "python" | "py" => Syntax::python(),
        "lua" => Syntax::lua(),
        "sql" => Syntax::sql(),
        "asm" => Syntax::asm(),
        "shell" | "sh" | "bash" => Syntax::shell(),
        _ => Syntax::default(),
    }
}
