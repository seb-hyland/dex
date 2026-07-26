use egui::{
    Color32, FontId, Pos2, Rect,
    text::{LayoutJob, TextWrapping},
};
use serde::{Deserialize, Serialize};
use utils::Reset;
use workspace::prelude::*;

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
        let start = ctx.constraints.continuation.unwrap_or(0) as usize;
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

        if ctx.constraints.can_request_wrap && !ctx.constraints.already_wrapped() {
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
        let can_wrap = ctx.constraints.can_request_wrap;

        let job = LayoutJob::simple(remaining.to_owned(), self.font.clone(), self.color, width);
        let galley = ctx.ui.ctx().fonts_mut(|f| f.layout_job(job));

        let first_row_height = galley.rows[0].height();
        if first_row_height > height {
            if !can_wrap || ctx.constraints.already_wrapped() {
                // No space to draw any more rows
                return DrawResult::Complete { region: None };
            }

            // Start the drawing process on a new line
            return DrawResult::Wrap {
                region: None,
                continuation: 0,
            };
        }

        let num_rows_to_layout = if can_wrap {
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

        if can_wrap && have_rows_remaining {
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
