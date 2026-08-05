use dex_core::prelude::*;
use egui::{
    Align, Color32, FontId, Frame, Layout, Margin, Pos2, Rect, TextEdit, UiBuilder,
    text::{LayoutJob, TextWrapping},
};
use egui_code_editor::{CodeEditor as CodeEditorWidget, ColorTheme, DEFAULT_THEMES, Syntax};
use serde::{Deserialize, Serialize};
use utils::{Reset, Transient, match_dyn};

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct Label {
    pub text: String,
    pub singleline: bool,

    pub font: FontId,
    pub color: Color32,
}

#[typetag::serde]
impl Node for Label {
    fn type_name(&self) -> String {
        "Text Label".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        // A continuation is a char offset
        let start = if let WrapConstraints::CanRequest { continuation, .. } = ctx.constraints.wrap
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

        let avail_w = ctx.constraints.x.map(|a| a.provided_value());
        let avail_h = ctx.constraints.y.map(|a| a.provided_value());

        if self.singleline {
            self.draw_singleline(ctx, &remaining, avail_w)
        } else {
            self.draw_multiline(ctx, &remaining, start, avail_w, avail_h)
        }
    }

    fn handle_action(&mut self, _r: Box<dyn ActionBody>) {}
}

impl Label {
    fn draw_singleline(
        &self,
        ctx: DrawContext,
        remaining: &str,
        avail_w: Option<f32>,
    ) -> DrawResult {
        let mut job =
            LayoutJob::simple_singleline(remaining.to_owned(), self.font.clone(), self.color);
        let galley = ctx.ui.ctx().fonts_mut(|f| f.layout_job(job.clone()));

        let fits = avail_w.is_none_or(|w| galley.rect.width() <= w);
        if fits {
            // We can render here normally!
            let origin: Pos2 = ctx
                .constraints
                .pos
                .to_top_left(galley.rect.size().into())
                .into();
            ctx.ui.painter().galley(origin, galley.clone(), self.color);
            return DrawResult::Complete {
                region: Some(galley.rect.translate(origin.to_vec2()).into()),
            };
        }

        if ctx.constraints.wrap.can_retry_on_newline() {
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

        let origin: Pos2 = ctx
            .constraints
            .pos
            .to_top_left(galley.rect.size().into())
            .into();
        ctx.ui.painter().galley(origin, galley.clone(), self.color);
        DrawResult::Complete {
            region: Some(galley.rect.translate(origin.to_vec2()).into()),
        }
    }

    fn draw_multiline(
        &self,
        ctx: DrawContext,
        remaining: &str,
        start: usize,
        avail_w: Option<f32>,
        avail_h: Option<f32>,
    ) -> DrawResult {
        let width = avail_w.unwrap_or(f32::INFINITY);
        let height = avail_h.unwrap_or(f32::INFINITY);
        let wrap_constraints = ctx.constraints.wrap;

        let job = LayoutJob::simple(remaining.to_owned(), self.font.clone(), self.color, width);
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
        let origin: Pos2 = ctx
            .constraints
            .pos
            .to_top_left(local_rect.size().into())
            .into();

        let mut clip_rect = ctx.ui.clip_rect();
        clip_rect.max.y = clip_rect.max.y.min(origin.y + local_rect.max.y);
        ctx.ui
            .painter()
            .with_clip_rect(clip_rect)
            .galley(origin, galley.clone(), self.color);

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

impl Requestable for Label {
    fn request(&self, _body: Box<dyn RequestBody>) -> Option<Box<dyn std::any::Any>> {
        None
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SetText {
    value: String,
}

#[typetag::serde]
impl ActionBody for SetText {}

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct LabelEditable {
    pub value: String,
    buf: Transient<String>,

    pub singleline: bool,
    pub shrink_to_text: bool,

    pub font: FontId,
    pub color: Color32,
}

#[typetag::serde]
impl Node for LabelEditable {
    fn type_name(&self) -> String {
        "Editable Label".to_owned()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let avail_w = ctx.constraints.x.map(|a| a.provided_value());
        let avail_h = ctx.constraints.y.map(|a| a.provided_value());

        // Measure the width of the text field
        let content = self.buf.val_or_else(|| self.value.clone()).clone();
        let measure_job = LayoutJob::simple_singleline(content, self.font.clone(), self.color);
        let galley = ctx.ui.ctx().fonts_mut(|f| f.layout_job(measure_job));
        let content_w = galley.rect.width();
        let row_h = galley.rows[0].height();

        // Horizontal fit --------------------------------------------------
        let min_w = row_h; // if field is empty, make a square the height of one row
        const CARET_PADDING: f32 = 2.0; // padding size so cursor remains visible
        let mut block_w = if self.shrink_to_text {
            (content_w + CARET_PADDING).max(min_w)
        } else {
            avail_w.unwrap_or((content_w + CARET_PADDING).max(min_w))
        };

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
            y: row_h,
        };
        let origin = ctx.constraints.pos.to_top_left(size);
        let rect = Rect::from_min_size(origin.into(), size.into());

        let mut buf_mut = self.buf.val_mut_or_else(|| self.value.clone());
        let editor_id = egui::Id::new(ctx.id);
        let editor = if self.singleline {
            TextEdit::singleline(&mut *buf_mut)
        } else {
            TextEdit::multiline(&mut *buf_mut)
        }
        .id(editor_id)
        .frame(Frame::NONE)
        .margin(Margin::ZERO)
        .font(self.font.clone())
        .text_color(self.color)
        .desired_width(block_w);

        // Update on focus loss
        if ctx
            .ui
            .memory_mut(|mem| mem.had_focus_last_frame(editor_id) && !mem.has_focus(editor_id))
            && let Some(v) = &*self.buf.val()
            && let Id::Workspace(uid) = ctx.id
        {
            ctx.workspace.submit_action(Action {
                dest: Some(uid),
                description: "Updated editable label's stored value on focus loss".into(),
                body: Box::new(SetText { value: v.clone() }),
            });
        }

        // Insert the widget flush inside the computed block.
        ctx.ui.scope_builder(
            UiBuilder::new()
                .max_rect(rect)
                .id_salt(ctx.id)
                .layout(Layout::left_to_right(Align::Min)),
            |ui| ui.add(editor),
        );

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, size)),
        }
    }

    fn handle_action(&mut self, r: Box<dyn ActionBody>) {
        match_dyn! { r,
            s: SetText => self.value = s.value,
            _ => {}
        }
    }
}

impl Requestable for LabelEditable {
    fn request(&self, _body: Box<dyn RequestBody>) -> Option<Box<dyn std::any::Any>> {
        None
    }
}

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct CodeEditor {
    pub value: String,
    buf: Transient<String>,

    pub font_size: f32,
    pub rows: usize,
    pub numlines: bool,

    /// Colour theme, matched by name against [`DEFAULT_THEMES`] (e.g. "Gruvbox")
    pub theme: String,
    /// Highlighting language (e.g. "rust", "python")
    pub language: String,
}

#[typetag::serde]
impl Node for CodeEditor {
    fn type_name(&self) -> String {
        "Code Editor".to_owned()
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
        let block_w = avail_w.unwrap_or(FALLBACK_WIDTH);
        let block_h = avail_h.unwrap_or(row_h * self.rows.max(1) as f32);
        let size = Vector {
            x: block_w,
            y: block_h,
        };
        let origin = ctx.constraints.pos.to_top_left(size);
        let rect = Rect::from_min_size(origin.into(), size.into());

        let syntax = syntax_for(&self.language);
        let editor_id = egui::Id::new(ctx.id);
        let mut editor = CodeEditorWidget::default()
            .with_id(editor_id)
            .with_fontsize(self.font_size)
            .with_rows(self.rows)
            .with_numlines(self.numlines)
            .with_theme(theme_for(&self.theme))
            .desired_width(block_w);

        // Update on focus loss
        if ctx
            .ui
            .memory_mut(|mem| mem.had_focus_last_frame(editor_id) && !mem.has_focus(editor_id))
            && let Some(v) = &*self.buf.val()
            && let Id::Workspace(uid) = ctx.id
        {
            ctx.workspace.submit_action(Action {
                dest: Some(uid),
                description: "Updated editable label's stored value on focus loss".into(),
                body: Box::new(SetText { value: v.clone() }),
            });
        }

        let mut buf_mut = self.buf.val_mut_or_else(|| self.value.clone());
        let drawn = ctx.ui.scope_builder(
            UiBuilder::new()
                .max_rect(rect)
                .id_salt(ctx.id)
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

    fn handle_action(&mut self, r: Box<dyn ActionBody>) {
        match_dyn! { r,
            s: SetText => self.value = s.value,
            _ => {}
        }
    }
}

impl Requestable for CodeEditor {
    fn request(&self, _body: Box<dyn RequestBody>) -> Option<Box<dyn std::any::Any>> {
        None
    }
}

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
