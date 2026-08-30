use dex_core::prelude::*;
use egui::Sense;
use serde::{Deserialize, Serialize};
use utils::Transient;

#[derive(Default)]
#[utils::dynamic_type]
#[utils::portable]
pub struct InteractionBox {
    pub senses_hover: bool,
    pub senses_clicks: bool,
    pub senses_drags: bool,

    cache: Transient<LastFrameInteractions>,
}

#[derive(Clone, Serialize, Deserialize)]
struct LastFrameInteractions {
    hovered: bool,
    contains_pointer: bool,
    clicked: bool,
    double_clicked: bool,
    secondary_clicked: bool,
    dragged: Option<Vector>,
    drag_pos: Option<ScreenPos>,
    /// Where the press that began the current drag landed.
    press_origin: Option<ScreenPos>,
    /// Pointer position while hovering (and during a click).
    hover_pos: Option<ScreenPos>,
    drag_stopped: bool,
}

#[utils::dynamic_methods]
impl InteractionBox {
    /// A sensor configured to sense the given gesture kinds.
    pub fn sensing(hover: bool, clicks: bool, drags: bool) -> Self {
        Self {
            senses_hover: hover,
            senses_clicks: clicks,
            senses_drags: drags,
            cache: Transient::default(),
        }
    }

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

#[utils::dynamic_node]
impl Node for InteractionBox {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "An Interaction Sensor".into()
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

        let origin = ctx.constraints.pos;
        let region = ScreenRegion::from_min_size(origin, size);

        let resp = ctx
            .ui
            .interact(region.into(), egui::Id::new(ctx.node.id), self.to_sense());
        // Where the pointer went down.
        let ui_press_origin = ctx.ui.input(|i| i.pointer.press_origin());

        self.cache.set(LastFrameInteractions {
            hovered: resp.hovered(),
            contains_pointer: resp.contains_pointer(),
            clicked: resp.clicked(),
            double_clicked: resp.double_clicked(),
            secondary_clicked: resp.secondary_clicked(),
            dragged: resp.dragged().then_some(resp.drag_delta().into()),
            drag_pos: resp
                .dragged()
                .then(|| resp.interact_pointer_pos())
                .flatten()
                .map(ScreenPos::from),
            press_origin: resp
                .dragged()
                .then_some(ui_press_origin)
                .flatten()
                .map(ScreenPos::from),
            hover_pos: resp.hover_pos().map(ScreenPos::from),
            drag_stopped: resp.drag_stopped(),
        });

        DrawResult::Complete {
            region: Some(region),
        }
    }
}

defhandlers! { InteractionBox {
    requests: [
        WasClicked => (this, _q): bool { this.cache.val().as_ref().is_some_and(|i| i.clicked) },
        // A click, consumed.
        TakeClicked => (this, _q): bool {
            this.cache
                .val_mut()
                .as_mut()
                .is_some_and(|i| ::std::mem::take(&mut i.clicked))
        },
        WasDoubleClicked => (this, _q): bool { this.cache.val().as_ref().is_some_and(|i| i.double_clicked) },
        WasRightClicked => (this, _q): bool { this.cache.val().as_ref().is_some_and(|i| i.secondary_clicked) },
        WasHovered => (this, _q): bool { this.cache.val().as_ref().is_some_and(|i| i.hovered) },
        // Pointer position over the sensor this frame (hovering or clicking).
        PointerPos => (this, _q): Option<ScreenPos> { this.cache.val().as_ref().and_then(|i| i.hover_pos) },
        ContainsPointer => (this, _q): bool { this.cache.val().as_ref().is_some_and(|i| i.contains_pointer) },
        WasDragged => (this, _q): Option<Vector> { this.cache.val().as_ref().and_then(|i| i.dragged) },
        // Live pointer position while a drag is in progress (for rubber-band feedback).
        DragPointerPos => (this, _q): Option<ScreenPos> { this.cache.val().as_ref().and_then(|i| i.drag_pos) },
        // Where the drag in progress started.
        DragStartPos => (this, _q): Option<ScreenPos> { this.cache.val().as_ref().and_then(|i| i.press_origin) },
        WasDragReleased => (this, _q): bool { this.cache.val().as_ref().is_some_and(|i| i.drag_stopped) },
    ],
}}
