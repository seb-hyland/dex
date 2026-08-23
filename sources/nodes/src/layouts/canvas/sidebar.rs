use dex_core::prelude::*;
use serde::{Deserialize, Serialize};
use utils::Reset;

use crate::{
    composites::button::Button,
    layouts::{
        HorizontalLayout,
        canvas::{
            layout::AddCanvasItem,
            nodes::shapes::{CanvasCircle, CanvasRect},
        },
        desktops::Desktops,
    },
    primitives::{
        interaction::WasClicked,
        text::{Label, LabelEditable},
    },
};

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct CanvasSidebar {
    pub desktops: NodeUid<Desktops>,
    buttons: Vec<NodeUid<Button>>,
}

impl CanvasSidebar {
    /// Labels for the option buttons.
    pub const OPTIONS: [&'static str; 3] = ["Text", "Rect", "Circle"];

    /// Build the sidebar and its option buttons into `ws`.
    pub fn build(ws: &Workspace, desktops: NodeUid<Desktops>) -> NodeUid<CanvasSidebar> {
        let buttons = Self::OPTIONS
            .iter()
            .map(|label| {
                Button::build_with(ws, Label::new((*label).to_owned()), |b| {
                    b.corner_radius = 5.0
                })
            })
            .collect();
        ws.insert_node(Self { desktops, buttons })
    }

    /// The insert action for the option at `index`.
    fn dispatch(&self, index: usize) -> Option<Action> {
        let dest = self.desktops.erase();
        let child: Arc<dyn Node> = match index {
            0 => Arc::new(LabelEditable::new("Text here".to_owned())),
            1 => Arc::new(CanvasRect),
            2 => Arc::new(CanvasCircle),
            _ => return None,
        };
        Some(Action {
            dest,
            description: "Insert new node".into(),
            body: Box::new(AddCanvasItem { child }),
        })
    }
}

#[typetag::serde]
impl Node for CanvasSidebar {
    fn type_name(&self) -> String {
        "Canvas Sidebar".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const GAP: f32 = 4.0;

        // Draw the option buttons in a vertical stack, then poll each.
        let buttons: Vec<NodeUid> = self.buttons.iter().map(|b| b.erase()).collect();
        let layout = HorizontalLayout {
            children: buttons
                .iter()
                .filter_map(|b_uid| ctx.get_workspace_node(*b_uid))
                .collect(),
            spacing: GAP,
            allow_wrap: true,
        };
        let result = ctx.draw_node(&layout, ctx.constraints);

        for (i, &btn) in self.buttons.iter().enumerate() {
            if ctx
                .node
                .workspace
                .send_request(btn.erase(), WasClicked)
                .unwrap_or(false)
                && let Some(action) = self.dispatch(i)
            {
                ctx.node.workspace.submit_action_dyn(action);
            }
        }

        result
    }

    fn on_delete(&self, ctx: NodeContext) {
        for btn in &self.buttons {
            ctx.workspace.delete_node(btn.erase());
        }
    }
}

defhandlers! { CanvasSidebar {} }
