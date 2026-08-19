use dex_core::prelude::*;
use egui::{Color32, LayerId, Stroke};
use serde::{Deserialize, Serialize};
use utils::{Reset, Transient};

use crate::{
    composites::button::Button,
    layouts::{
        canvas::{layout::Canvas, sidebar::CanvasSidebar},
        horizontal_dnd::{AddChild, HorizontalDnD},
        horizontal_layout,
    },
    primitives::{
        interaction::{InteractionBox, WasClicked, WasDoubleClicked, WasDragged, WasHovered},
        shapes::Rect,
        text::{IsInteractive, Label, LabelEditable, SetInteractive},
    },
};

/**
   The workspace root.

   This owns several [`Canvas`] desktops, with one active at a time. It draws a
   sidebar (adds items to the active canvas), a drag-and-drop tab bar, and the
   active canvas.
*/
#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct Desktops {
    tab_bar: NodeUid<HorizontalDnD>,
    active: Option<NodeUid<Canvas>>,
    sidebar: NodeUid<CanvasSidebar>,
    add_button: NodeUid<Button>,
    divider: NodeUid<InteractionBox>,

    sidebar_width: f32,
    pending_sidebar_width: Transient<f32>,
}

impl Desktops {
    /// Build a fresh workspace with a [`Desktops`] instance as its root.
    pub fn new_workspace() -> Workspace {
        let mut ws = Workspace::new_empty();
        let root = Desktops::build(&ws);
        // Drain the queued inserts so the tree is live before the first frame.
        ws.process_pending();
        ws.set_root(root.erase());
        ws
    }

    /// Build the desktops root (and its whole subtree) into `ws`.
    pub fn build(ws: &Workspace) -> NodeUid<Desktops> {
        let id = NodeUid::<Desktops>::new_workspace();

        let canvas = Canvas::build(ws);
        let tab = DesktopTabView::build(ws, canvas, id, "Canvas 1".to_owned());
        let tab_bar = HorizontalDnD::build(ws, vec![tab.erase()], TAB_SPACING);
        let sidebar = CanvasSidebar::build(ws, id);
        let add_button = Button::build(ws, Label::new("+".to_owned()));
        let divider = ws.insert_node(Box::new(InteractionBox::sensing(true, false, true)));

        ws.insert_node_at(
            id,
            Box::new(Desktops {
                tab_bar,
                active: Some(canvas),
                sidebar,
                add_button,
                divider,
                sidebar_width: 200.0,
                pending_sidebar_width: Transient::default(),
            }),
        );
        id
    }
}

const DIVIDER_W: f32 = 6.0;
const SIDEBAR_MIN: f32 = 120.0;
const SIDEBAR_MAX: f32 = 500.0;
const TAB_BAR_H: f32 = 42.0;
const TAB_SPACING: f32 = 6.0;

#[typetag::serde]
impl Node for Desktops {
    fn type_name(&self) -> String {
        "Desktops".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let avail_w = ctx.constraints.x.map(|a| a.provided_value()).unwrap_or(0.0);
        let avail_h = ctx.constraints.y.map(|a| a.provided_value()).unwrap_or(0.0);
        let origin = ctx.constraints.pos.to_top_left(Vector {
            x: avail_w,
            y: avail_h,
        });

        // Paint the whole screen background white.
        ctx.ui.layer_painter(LayerId::background()).rect_filled(
            ScreenRegion::from_min_size(
                origin,
                Vector {
                    x: avail_w,
                    y: avail_h,
                },
            )
            .into(),
            0.0,
            Color32::WHITE,
        );

        // Draw sidebar ----------------------------------------
        let pending = *self.pending_sidebar_width.val();
        let sidebar_w = pending
            .unwrap_or(self.sidebar_width)
            .clamp(SIDEBAR_MIN, SIDEBAR_MAX);

        let divider_x = origin.x + sidebar_w;
        let right_x = divider_x + DIVIDER_W;
        let right_w = (avail_w - sidebar_w - DIVIDER_W).max(0.0);
        let right_origin = ScreenPos {
            x: right_x,
            y: origin.y,
        };

        ctx.draw_workspace_node(
            self.sidebar,
            DrawConstraints {
                pos: PositionConstraint::TopLeft(origin),
                x: Some(AxisConstraint::Exactly(sidebar_w)),
                y: Some(AxisConstraint::Exactly(avail_h)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );

        // Tab bar, then the add-canvas button, laid out in a row.
        horizontal_layout(
            &mut ctx,
            &[self.tab_bar.erase(), self.add_button.erase()],
            TAB_SPACING,
            false,
            DrawConstraints {
                pos: PositionConstraint::TopLeft(
                    right_origin
                        + Vector {
                            x: TAB_SPACING,
                            y: TAB_SPACING,
                        },
                ),
                x: Some(AxisConstraint::AtMost(
                    (right_w - 2.0 * TAB_SPACING).max(0.0),
                )),
                y: Some(AxisConstraint::AtMost((TAB_BAR_H - TAB_SPACING).max(0.0))),
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );
        if ctx
            .node
            .workspace
            .send_request(self.add_button.erase(), WasClicked)
            .unwrap_or(false)
        {
            ctx.submit_action_for_self::<Self, _>(AddCanvas, "Add canvas");
        }

        // Draw active canvas -------------------------------------------------
        if let Some(active) = self.active {
            ctx.draw_workspace_node(
                active,
                DrawConstraints {
                    pos: PositionConstraint::TopLeft(ScreenPos {
                        x: right_x,
                        y: origin.y + TAB_BAR_H,
                    }),
                    x: Some(AxisConstraint::Exactly(right_w)),
                    y: Some(AxisConstraint::Exactly((avail_h - TAB_BAR_H).max(0.0))),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
            );
        }

        // Draw sidebar splitter ----------------------------------------
        ctx.draw_workspace_node(
            self.divider,
            DrawConstraints {
                pos: PositionConstraint::TopLeft(ScreenPos {
                    x: divider_x,
                    y: origin.y,
                }),
                x: Some(AxisConstraint::Exactly(DIVIDER_W)),
                y: Some(AxisConstraint::Exactly(avail_h)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );

        let divider_active = pending.is_some()
            || ctx
                .node
                .workspace
                .send_request(self.divider, WasHovered)
                .unwrap_or(false);
        let divider_color = if divider_active {
            Color32::from_gray(150)
        } else {
            Color32::from_gray(220)
        };
        ctx.ui.painter().rect_filled(
            ScreenRegion::from_min_size(
                ScreenPos {
                    x: divider_x + DIVIDER_W * 0.5 - 0.5,
                    y: origin.y,
                },
                Vector { x: 1.0, y: avail_h },
            )
            .into(),
            0.0,
            divider_color,
        );

        if let Some(delta) = ctx
            .node
            .workspace
            .send_request(self.divider, WasDragged)
            .flatten()
        {
            let base = pending.unwrap_or(self.sidebar_width);
            self.pending_sidebar_width
                .set((base + delta.x).clamp(SIDEBAR_MIN, SIDEBAR_MAX));
        } else if let Some(final_w) = pending {
            *self.pending_sidebar_width.val_mut() = None;
            ctx.submit_action_for_self::<Self, _>(
                SetSidebarWidth { width: final_w },
                "Resized sidebar",
            );
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(
                origin,
                Vector {
                    x: avail_w,
                    y: avail_h,
                },
            )),
        }
    }

    fn deref_target(&self) -> Option<NodeUid> {
        // Actions/requests we do not handle (e.g. sidebar inserts) fall through
        // to the active canvas. This is the "send to the active canvas" API.
        self.active.map(|canvas| canvas.erase())
    }
}

defhandlers! { Desktops {
    actions: [
        AddCanvas => (this, _a, ctx) {
            let canvas = Canvas::build(ctx.workspace);
            let tab = DesktopTabView::build(ctx.workspace, canvas, ctx.id.cast(), "New canvas".to_owned());
            ctx.workspace.submit_action(
                this.tab_bar,
                "Add tab",
                AddChild { child: tab.erase() },
            );
            this.active = Some(canvas);
        },
        SetActive { canvas: NodeUid<Canvas> } => (this, s) {
            this.active = Some(s.canvas);
        },
        SetSidebarWidth { width: f32 } => (this, s) {
            this.sidebar_width = s.width;
        },
    ],
    requests: [
        ActiveCanvas => (this, _q): Option<NodeUid<Canvas>> { this.active },
    ],
}}

/// Display node for a single tab (labelled by canvas name).
#[derive(Clone, Copy, Reset, Serialize, Deserialize)]
struct DesktopTabView {
    canvas: NodeUid<Canvas>,
    name: NodeUid<LabelEditable>,
    parent: NodeUid<Desktops>,
    /// Click/double-click sensor over the whole tab.
    sensor: NodeUid<InteractionBox>,
}

impl DesktopTabView {
    /// Build a tab (its editable name + click sensor) into `ws`.
    fn build(
        ws: &Workspace,
        canvas: NodeUid<Canvas>,
        parent: NodeUid<Desktops>,
        name_text: String,
    ) -> NodeUid<DesktopTabView> {
        let name = ws.insert_node(Box::new(LabelEditable::click_to_edit(name_text)));
        let sensor = ws.insert_node(Box::new(InteractionBox::sensing(false, true, false)));
        ws.insert_node(Box::new(Self {
            canvas,
            name,
            parent,
            sensor,
        }))
    }
}

#[typetag::serde]
impl Node for DesktopTabView {
    fn type_name(&self) -> String {
        "Canvas Tab".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const PAD_X: f32 = 10.0;
        const PAD_Y: f32 = 5.0;

        let avail_w = ctx
            .constraints
            .x
            .map(|a| a.provided_value())
            .unwrap_or(f32::INFINITY);
        let avail_h = ctx
            .constraints
            .y
            .map(|a| a.provided_value())
            .unwrap_or(f32::INFINITY);
        let origin = ctx.constraints.pos.to_top_left(Vector::splat(0.0));

        let editing = ctx
            .node
            .workspace
            .send_request(self.name, IsInteractive)
            .unwrap_or(false);
        let active = ctx
            .node
            .workspace
            .send_request(self.parent, ActiveCanvas)
            .flatten()
            == Some(self.canvas);

        // The editable name, inset by the padding.
        let name_res = ctx.draw_workspace_node(
            self.name,
            DrawConstraints {
                pos: PositionConstraint::TopLeft(origin + Vector { x: PAD_X, y: PAD_Y }),
                x: Some(AxisConstraint::AtMost((avail_w - 2.0 * PAD_X).max(0.0))),
                y: Some(AxisConstraint::AtMost((avail_h - 2.0 * PAD_Y).max(0.0))),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );
        let name_size = name_res
            .and_then(|r| r.region())
            .map(|r| r.size())
            .unwrap_or(Vector { x: 48.0, y: 18.0 });

        let tab_size = Vector {
            x: name_size.x + 2.0 * PAD_X,
            y: name_size.y + 2.0 * PAD_Y,
        };

        // Outline; the active tab gets a stronger accent.
        let border = if active {
            Stroke::new(1.5, Color32::from_rgb(70, 130, 180))
        } else {
            Stroke::new(1.0, Color32::from_gray(210))
        };
        let outline = Rect {
            size: tab_size,
            corner_radius: 4.0,
            fill_color: Color32::TRANSPARENT,
            border,
        };
        outline.paint(ctx.ui.painter(), origin);

        // Click/double-click sensor over the whole tab when not editing.
        if !editing {
            ctx.draw_workspace_node(
                self.sensor,
                DrawConstraints {
                    pos: PositionConstraint::TopLeft(origin),
                    x: Some(AxisConstraint::Exactly(tab_size.x)),
                    y: Some(AxisConstraint::Exactly(tab_size.y)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: false,
                },
            );

            let ws = ctx.node.workspace;
            if ws
                .send_request(self.sensor, WasDoubleClicked)
                .unwrap_or(false)
            {
                // Start renaming, and make sure this canvas is the active one.
                ws.submit_action(self.name, "Edit canvas name", SetInteractive { on: true });
                ws.submit_action(
                    self.parent,
                    "Activated canvas",
                    SetActive {
                        canvas: self.canvas,
                    },
                );
            } else if ws.send_request(self.sensor, WasClicked).unwrap_or(false) {
                ws.submit_action(
                    self.parent,
                    "Activated canvas",
                    SetActive {
                        canvas: self.canvas,
                    },
                );
            }
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, tab_size)),
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.name.erase());
        ctx.workspace.delete_node(self.sensor.erase());
    }
}

defhandlers! { DesktopTabView {} }
