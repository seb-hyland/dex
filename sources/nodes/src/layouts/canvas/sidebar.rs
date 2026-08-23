use dex_core::prelude::*;

use crate::{
    composites::{button::Button, lambda::Lambda},
    layouts::{
        HorizontalLayout, LayoutChild,
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

#[utils::dynamic_type]
#[utils::portable]
pub struct CanvasSidebar {
    desktops: NodeUid<Desktops>,
    buttons: Vec<NodeUid<Button>>,
}

#[utils::dynamic_methods]
impl CanvasSidebar {
    /// Labels for the option buttons, in order. The button at index `i` inserts
    /// the node produced by [`CanvasSidebar::dispatch`] for that index.
    pub const OPTIONS: [&'static str; 4] = ["Text", "Rect", "Circle", "Lambda"];

    /// Build the sidebar and its option buttons into `ws`.
    pub fn build(ws: WorkspaceActionHandle, desktops: NodeUid<Desktops>) -> NodeUid<CanvasSidebar> {
        let buttons = Self::OPTIONS
            .iter()
            .map(|label| {
                Button::build_with(ws.clone(), Label::new((*label).to_owned()), |b| {
                    b.corner_radius = 5.0
                })
            })
            .collect();
        ws.insert_node(Self { desktops, buttons })
    }

    /// The insert action for the option at `index`.
    fn dispatch(&self, index: usize, ws: WorkspaceActionHandle) -> Option<Action> {
        let dest = self.desktops.erase();
        let child: Arc<dyn Node> = match index {
            0 => Arc::new(LabelEditable::new("Text here".to_owned())),
            1 => Arc::new(CanvasRect),
            2 => Arc::new(CanvasCircle),
            3 => Arc::new(Lambda::new(ws)),
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
        const GAP: f32 = 10.0;
        const PADDING: f32 = 5.0;

        // Draw the option buttons in a vertical stack, then poll each.
        let layout = HorizontalLayout {
            children: self.buttons.iter().map(|b| LayoutChild::from(*b)).collect(),
            spacing: GAP,
            allow_wrap: true,
        };
        let result = ctx.draw_node(
            &layout,
            ctx.constraints.shrunk_by_per_side(PADDING, PADDING),
        );

        for (i, &btn) in self.buttons.iter().enumerate() {
            if ctx
                .node
                .workspace
                .send_request(btn.erase(), WasClicked)
                .unwrap_or(false)
                && let Some(action) = self.dispatch(i, ctx.node.workspace.action_handle())
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
