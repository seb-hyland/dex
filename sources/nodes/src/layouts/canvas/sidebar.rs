use dex_core::prelude::*;
use serde::{Deserialize, Serialize};
use utils::Reset;

use crate::{
    composites::button::Button,
    layouts::{
        LayoutChild,
        canvas::{
            layout::AddCanvasItem,
            nodes::shapes::{CanvasCircle, CanvasRect},
        },
        desktops::Desktops,
        horizontal::HorizontalLayout,
    },
    primitives::text::{Label, LabelEditable},
};

#[derive(Clone, Copy, Reset, Serialize, Deserialize)]
pub struct CanvasSidebar {
    /// The owning [`Desktops`]. Insert actions are addressed here and forwarded
    /// to its active canvas via [`Desktops`]'s deref target.
    pub desktops: NodeUid<Desktops>,
}

#[typetag::serde]
impl Node for CanvasSidebar {
    fn type_name(&self) -> String {
        "Canvas Sidebar".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let children: Vec<Box<dyn Node>> = vec![
            Box::new(LabelEditable::new("Text here".to_owned())),
            Box::new(CanvasRect),
            Box::new(CanvasCircle),
        ];
        let options = children
            .into_iter()
            .map(|child| {
                LayoutChild::Local(Box::new(Button::new(
                    Label::new(child.type_name()),
                    Action {
                        dest: self.desktops.erase(),
                        description: "Insert new node".into(),
                        body: Box::new(AddCanvasItem { child }),
                    },
                )))
            })
            .collect();

        let layout = HorizontalLayout {
            children: options,
            allow_wrap: true,
            wrap_spacing: 4.0,
        };
        ctx.draw_node(
            &layout,
            NodeUid::new_local(ctx.node.id, "sidebar items"),
            ctx.constraints,
        )
    }
}

defhandlers! { CanvasSidebar {} }
