use dex_core::prelude::*;
use egui::Sense;
use serde::{Deserialize, Serialize};
use utils::{Reset, Transient};

#[derive(Clone, Default, Reset, Serialize, Deserialize)]
pub struct InteractionBox {
    pub senses_hover: bool,
    pub senses_clicks: bool,
    pub senses_drags: bool,
    cache: Transient<LastFrameInteractions>,
}

#[derive(Clone, Serialize, Deserialize)]
struct LastFrameInteractions {
    hovered: bool,
    clicked: bool,
    dragged: Option<Vector>,
}

impl InteractionBox {
    fn to_sense(&self) -> Sense {
        let hover = if self.senses_hover {
            Sense::HOVER
        } else {
            Sense::empty()
        };

        let click = if self.senses_clicks {
            Sense::CLICK
        } else {
            Sense::empty()
        };

        let drag = if self.senses_drags {
            Sense::DRAG
        } else {
            Sense::empty()
        };

        hover | click | drag
    }
}

#[typetag::serde]
impl Node for InteractionBox {
    fn type_name(&self) -> String {
        "Interaction Sensor".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let Some(x) = ctx.constraints.x else {
            // Cannot draw; unbounded
            return DrawResult::Complete { region: None };
        };
        let Some(y) = ctx.constraints.y else {
            // Cannot draw; unbounded
            return DrawResult::Complete { region: None };
        };
        let size = Vector {
            x: x.provided_value(),
            y: y.provided_value(),
        };

        let origin = ctx.constraints.pos.to_top_left(size);
        let region = ScreenRegion::from_min_size(origin, size);

        let resp = ctx
            .ui
            .interact(region.into(), egui::Id::new(ctx.id), self.to_sense());
        self.cache.set(LastFrameInteractions {
            hovered: resp.hovered(),
            clicked: resp.clicked(),
            dragged: resp.dragged().then_some(resp.drag_delta().into()),
        });

        DrawResult::Complete {
            region: Some(region),
        }
    }
}

defhandlers! { InteractionBox {
    requests: [
        WasClicked => (this, _q): bool { this.cache.val().as_ref().is_some_and(|i| i.clicked) },
        WasHovered => (this, _q): bool { this.cache.val().as_ref().is_some_and(|i| i.hovered) },
        WasDragged => (this, _q): Option<Vector> { this.cache.val().as_ref().and_then(|i| i.dragged) },
    ],
}}
