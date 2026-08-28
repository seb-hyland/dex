use dex_core::prelude::*;

use crate::{
    composites::{
        button::Button,
        lambda::{CanvasLambda, Lambda},
    },
    layouts::{
        HorizontalLayout, LayoutChild,
        canvas::{
            layout::AddCanvasItem,
            nodes::shapes::{CanvasCircle, CanvasRect},
        },
        desktops::Desktops,
    },
    primitives::{
        file_browser::FileBrowser,
        interaction::WasClicked,
        text::{CodeEditor, GetCommittedText, Label, LabelEditable},
        typst::TypstEditor,
    },
};

#[utils::dynamic_type]
#[utils::portable]
pub struct CanvasSidebar {
    desktops: NodeUid<Desktops>,
    buttons: Vec<NodeUid<Button>>,

    python_prelude: NodeUid<CodeEditor>,
}

#[utils::dynamic_methods]
impl CanvasSidebar {
    /// Labels for the option buttons, in order. The button at index `i` inserts
    /// the node produced by [`CanvasSidebar::dispatch`] for that index.
    pub const OPTIONS: [&'static str; 7] = [
        "Text",
        "Rect",
        "Circle",
        "Typst",
        "Lambda",
        "Canvas Lambda",
        "File",
    ];

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
        let python_prelude = ws.insert_node(CodeEditor::new(String::new(), "python".to_owned()));
        ws.insert_node(Self {
            desktops,
            buttons,
            python_prelude,
        })
    }

    /// The insert action for the option at `index`.
    fn dispatch(&self, index: usize, ws: WorkspaceActionHandle) -> Option<Action> {
        let dest = self.desktops.erase();
        const DEFAULT: Vector = Vector { x: 160.0, y: 40.0 };
        let (child, size): (Arc<dyn Node>, Vector) = match index {
            0 => (
                Arc::new(LabelEditable::new("Text here".to_owned())),
                DEFAULT,
            ),
            1 => (Arc::new(CanvasRect), DEFAULT),
            2 => (Arc::new(CanvasCircle), DEFAULT),
            3 => (
                Arc::new(TypstEditor::new(ws)),
                Vector { x: 280.0, y: 220.0 },
            ),
            4 => (Arc::new(Lambda::new(ws)), Vector { x: 420.0, y: 340.0 }),
            5 => (
                Arc::new(CanvasLambda::new(ws)),
                Vector { x: 280.0, y: 220.0 },
            ),
            6 => (
                Arc::new(FileBrowser::new(ws)),
                Vector { x: 320.0, y: 240.0 },
            ),
            _ => return None,
        };
        Some(Action {
            dest,
            description: "Insert new node".into(),
            body: Box::new(AddCanvasItem { child, size }),
        })
    }
}

#[utils::dynamic_node]
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

defhandlers! {
    CanvasSidebar {
        requests: [
            PythonPrelude => (this, _q, ctx): String {
                ctx.workspace.send_request(this.python_prelude, GetCommittedText {}).unwrap_or_default()
            },
        ]
    }
}
