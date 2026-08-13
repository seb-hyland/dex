use dex_core::prelude::*;
use egui::{Color32, LayerId, Stroke};
use serde::{Deserialize, Serialize};
use utils::{Reset, Transient};

use crate::{
    composites::button::Button,
    layouts::{
        canvas::{layout::Canvas, sidebar::CanvasSidebar},
        horizontal_dnd::{AddChild, HorizontalDnD},
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

    sidebar_width: f32,
    pending_sidebar_width: Transient<f32>,
}

impl Desktops {
    /// Build a fresh workspace with a [`Desktops`] instance pre-inserted.
    pub fn new_workspace() -> Workspace {
        let mut workspace = Workspace::new_empty();

        let desktops_id = NodeUid::<Desktops>::new_workspace();

        let canvas = workspace.insert_node_now(Box::new(Canvas::default()));
        let name = workspace.insert_node_now(Box::new(LabelEditable::click_to_edit(
            "Canvas 1".to_owned(),
        )));
        let tab = workspace.insert_node_now(Box::new(DesktopTabView {
            canvas,
            name,
            parent: desktops_id,
        }));
        let tab_bar =
            workspace.insert_node_now(Box::new(HorizontalDnD::new(vec![tab.erase()], TAB_SPACING)));

        workspace.insert_node_now_at(
            desktops_id,
            Box::new(Desktops {
                tab_bar,
                active: Some(canvas),
                sidebar_width: 200.0,
                pending_sidebar_width: Transient::default(),
            }),
        );

        workspace.set_root(desktops_id.erase());
        workspace
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

        let sidebar = CanvasSidebar {
            desktops: ctx.node.id.cast(),
        };
        ctx.draw_node(
            &sidebar,
            NodeUid::new_local(ctx.node.id, "sidebar"),
            DrawConstraints {
                pos: PositionConstraint::TopLeft(origin),
                x: Some(AxisConstraint::Exactly(sidebar_w)),
                y: Some(AxisConstraint::Exactly(avail_h)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );

        // Tab bar ----------------------------------------
        let tab_bar_res = ctx.draw_workspace_node(
            self.tab_bar,
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

        // Add tab button ----------------------------------------
        let tabs_right = tab_bar_res
            .and_then(|r| r.region())
            .map(|r| r.min.x + r.size().x)
            .unwrap_or(right_origin.x + TAB_SPACING);
        let add_button = Button::new(
            Label::new("+".to_owned()),
            Action {
                dest: ctx.node.id,
                description: "Add canvas".into(),
                body: Box::new(AddCanvas),
            },
        );
        ctx.draw_node(
            &add_button,
            NodeUid::new_local(ctx.node.id, "add canvas"),
            DrawConstraints {
                pos: PositionConstraint::TopLeft(ScreenPos {
                    x: tabs_right + TAB_SPACING,
                    y: right_origin.y + TAB_SPACING,
                }),
                x: Some(AxisConstraint::AtMost(60.0)),
                y: Some(AxisConstraint::AtMost((TAB_BAR_H - TAB_SPACING).max(0.0))),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );

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
        let mut divider = InteractionBox::default();
        divider.senses_drags = true;
        divider.senses_hover = true;
        ctx.draw_node(
            &divider,
            NodeUid::new_local(ctx.node.id, "sidebar divider"),
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

        let divider_active =
            pending.is_some() || divider.request(WasHovered, ctx.node).unwrap_or(false);
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

        if let Some(delta) = divider.request(WasDragged, ctx.node).flatten() {
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
            let canvas = ctx.workspace.insert_node(Box::new(Canvas::default()));
            let name = ctx.workspace.insert_node(Box::new(
                LabelEditable::click_to_edit("New canvas".to_owned())
            ));
            let tab = ctx.workspace.insert_node(Box::new(DesktopTabView {
                canvas,
                name,
                parent: ctx.id.cast(),
            }));
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
        ctx.draw_node(
            &outline,
            NodeUid::new_local(ctx.node.id, "tab border"),
            DrawConstraints {
                pos: PositionConstraint::TopLeft(origin),
                x: Some(AxisConstraint::Exactly(tab_size.x)),
                y: Some(AxisConstraint::Exactly(tab_size.y)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );

        // Click/double-click sensor over the whole tab when not editing.
        if !editing {
            let mut sensor = InteractionBox::default();
            sensor.senses_clicks = true;
            ctx.draw_node(
                &sensor,
                NodeUid::new_local(ctx.node.id, "tab click"),
                DrawConstraints {
                    pos: PositionConstraint::TopLeft(origin),
                    x: Some(AxisConstraint::Exactly(tab_size.x)),
                    y: Some(AxisConstraint::Exactly(tab_size.y)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: false,
                },
            );

            if sensor.request(WasDoubleClicked, ctx.node).unwrap_or(false) {
                // Start renaming, and make sure this canvas is the active one.
                ctx.node.workspace.submit_action(
                    self.name,
                    "Edit canvas name",
                    SetInteractive { on: true },
                );
                ctx.node.workspace.submit_action(
                    self.parent,
                    "Activated canvas",
                    SetActive {
                        canvas: self.canvas,
                    },
                );
            } else if sensor.request(WasClicked, ctx.node).unwrap_or(false) {
                ctx.node.workspace.submit_action(
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
}

defhandlers! { DesktopTabView {} }
