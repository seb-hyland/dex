use dex_core::prelude::*;
use egui::{Id, Popup, PopupCloseBehavior, PopupKind, RectAlign, Sense};
use utils::Transient;

use crate::{
    composites::button::Button,
    layouts::{
        LayoutChild, canvas::layout::PlaceOnCanvas, desktops::AddToBackpack, mirror::Mirror,
        vertical::VerticalLayout,
    },
    primitives::{interaction::TakeClicked, shapes::Rect, text::Label},
};

static VAR_NAME: f32 = 8.0;
/// Where the lens sits relative to the node it belongs to.
const HANDLE_OFFSET: f32 = VAR_NAME;
const HANDLE_SIZE: Vector = Vector { x: 14.0, y: 22.0 };
/// The menu's width. Fixed, as the popup sizes itself to its contents.
const MENU_WIDTH: f32 = 180.0;
/// How far the pointer may stray from the menu before it closes.
const MENU_SLACK: f32 = 36.0;
/// A stable egui id: there is only ever one handle.
const HANDLE_ID: &str = "dex_halo_handle";

/// Where the lens last sat, so an open menu keeps its place.
#[derive(Clone)]
pub struct HandleState {
    pub target: NodeUid,
    pub region: ScreenRegion,
}

#[utils::portable]
pub struct Inspector {
    /// The open menu's contents.
    inspector: Option<NodeUid>,
    /// The node `inspector` was built for, so a new target rebuilds it.
    #[uid_ref]
    inspected: Option<NodeUid>,
    /// This frame's handle interaction.
    handle: Transient<HandleState>,
}

impl Inspector {
    pub fn new() -> Inspector {
        Inspector {
            inspector: None,
            inspected: None,
            handle: Transient::default(),
        }
    }
}

impl Default for Inspector {
    fn default() -> Self {
        Self::new()
    }
}

#[utils::dynamic_node(skip)]
impl Node for Inspector {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "An Inspector".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let ws = ctx.node.workspace;

        // While a menu is open the target is sticky.
        let found = ws.inspect_target();
        let held = self.handle.val().clone();

        // An open menu holds the target.
        let sticky = match (self.inspector, self.inspected) {
            (Some(_), Some(inspected)) => Some(inspected),
            _ => None,
        };

        let shown = match sticky {
            Some(target) if ws.get_node(target).is_some() => {
                // Prefer this frame's region; fall back to where it last drew.
                let region = found
                    .as_ref()
                    .filter(|t| t.node == target)
                    .map(|t| t.region)
                    .or_else(|| {
                        held.as_ref()
                            .filter(|h| h.target == target)
                            .map(|h| h.region)
                    });
                region.map(|region| (target, region))
            }
            _ => found.as_ref().map(|t| (t.node, t.region)),
        };

        let Some((target, region)) = shown else {
            self.handle.val_mut().take();
            if self.inspector.is_some() {
                ctx.submit_action_for_self::<Self, _>(CloseInspector, "Closed inspector");
            }
            return DrawResult::Complete { region: None };
        };

        // Beside the node, in the margin, like the canvas handle it replaces.
        let handle_region = ScreenRegion::from_min_size(
            ScreenPos {
                x: region.min.x - HANDLE_OFFSET,
                y: region.min.y - HANDLE_OFFSET,
            },
            HANDLE_SIZE,
        );
        // The inspector owns its chrome, so it interacts directly.
        let resp = ctx
            .ui
            .interact(handle_region.into(), Id::new(HANDLE_ID), Sense::CLICK);
        let engaged =
            resp.hovered() || resp.is_pointer_button_down_on() || self.inspector.is_some();

        Rect {
            size: HANDLE_SIZE,
            corner_radius: 4.0,
            fill_color: if engaged {
                Color::gray(228)
            } else {
                Color::rgba(0, 0, 0, 12)
            },
            border: if engaged {
                Stroke::new(1.0, Color::gray(160))
            } else {
                Stroke::NONE
            },
            stroke_kind: StrokeKind::Middle,
        }
        .paint(ctx.ui.painter(), handle_region.min);

        // A magnifying glass: this inspects, it does not move anything.
        let ink = if engaged {
            egui::Color32::from_gray(70)
        } else {
            egui::Color32::from_gray(140)
        };
        let lens_radius = 3.6;
        let lens_centre = handle_region.min
            + Vector {
                x: HANDLE_SIZE.x * 0.44,
                y: HANDLE_SIZE.y * 0.42,
            };
        let painter = ctx.ui.painter();
        painter.circle_stroke(lens_centre.into(), lens_radius, egui::Stroke::new(1.3, ink));
        let reach = lens_radius * 0.72;
        painter.line_segment(
            [
                (lens_centre + Vector::splat(reach)).into(),
                (lens_centre + Vector::splat(reach + 3.2)).into(),
            ],
            egui::Stroke::new(1.3, ink),
        );

        self.handle.set(HandleState { target, region });

        // Opened by a click on the lens, and registered every frame.
        let inspector = self.inspector;
        let node = ctx.node;
        // Beside the lens, on a side chosen from where the lens is.
        let content = ctx.ui.ctx().content_rect();
        let align = if handle_region.min.x - MENU_WIDTH >= content.left() {
            RectAlign::LEFT_START
        } else {
            RectAlign::RIGHT_START
        };
        let popup = Popup::menu(&resp)
            .kind(PopupKind::Tooltip)
            .close_behavior(PopupCloseBehavior::IgnoreClicks)
            .align(align)
            .align_alternatives(&[])
            .width(MENU_WIDTH);
        let was_open = popup.is_open();
        let popup_id = popup.get_id();

        // Last frame's rect, in case this frame's is not reported.
        let previous_rect = popup.get_popup_rect();

        let shown = popup.show(|ui| {
            let Some(inspector) = inspector else {
                return;
            };
            let constraints = DrawConstraints {
                pos: ui.cursor().min.into(),
                x: Some(AxisConstraint::Exactly(MENU_WIDTH)),
                y: None,
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            };
            let mut menu_ctx = DrawContext::for_ui(node, constraints, ui);
            let drawn = menu_ctx.draw_workspace_node(inspector, constraints);
            if let Some(size) = drawn.and_then(|r| r.region()).map(|r| r.size())
                && size.x.is_finite()
                && size.y.is_finite()
            {
                ui.allocate_space(size.into());
            }
        });

        // Close once the pointer has left the menu for good.
        if was_open {
            let pointer = ctx.ui.ctx().pointer_latest_pos();
            let within =
                |rect: egui::Rect| pointer.is_some_and(|p| rect.expand(MENU_SLACK).contains(p));
            let menu_rect = shown.as_ref().map(|r| r.response.rect).or(previous_rect);
            let holding = menu_rect.is_some_and(within) || within(handle_region.into());
            if !holding {
                Popup::close_id(ctx.ui.ctx(), popup_id);
            }
        }

        // Build on the click that opened it; tear down once it has closed.
        if was_open && self.inspected != Some(target) {
            ctx.submit_action_for_self::<Self, _>(
                OpenInspector {
                    node: target,
                    size: region.size(),
                },
                "Opened inspector",
            );
        } else if !was_open && self.inspector.is_some() {
            ctx.submit_action_for_self::<Self, _>(CloseInspector, "Closed inspector");
        }

        DrawResult::Complete {
            region: Some(handle_region),
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
        if let Some(inspector) = self.inspector {
            ctx.workspace.delete_node(inspector);
        }
    }
}

/// Copy and Mirror, onto the canvas or into the backpack, for a node that can belong on a canvas.
#[utils::dynamic_type]
#[utils::portable]
pub struct PlacementCommands {
    /// The node these place a copy or a mirror of.
    #[uid_ref]
    target: NodeUid,
    /// The size a placed copy should take when it has no canvas layout of its own.
    size: Vector,
    copy_button: NodeUid<Button>,
    mirror_button: NodeUid<Button>,
    keep_copy_button: NodeUid<Button>,
    keep_mirror_button: NodeUid<Button>,
    column: NodeUid<VerticalLayout>,
}

#[utils::dynamic_methods]
impl PlacementCommands {
    /// Build the Copy and Mirror pair for `target`, to sit in its inspector.
    pub fn build(
        ws: WorkspaceActionHandle,
        target: NodeUid,
        size: Vector,
    ) -> NodeUid<PlacementCommands> {
        let command = |label: &str| {
            Button::build_with(ws.clone(), Label::new(label.to_owned()), |b| {
                b.padding = 4.0;
                b.corner_radius = 3.0;
                b.border = Stroke::NONE;
                b.fill_width = true;
            })
        };
        let copy_button = command("Copy");
        let mirror_button = command("Mirror");
        let keep_copy_button = command("Copy to Backpack");
        let keep_mirror_button = command("Mirror to Backpack");
        let column = VerticalLayout::build(
            ws.clone(),
            vec![
                copy_button.erase(),
                mirror_button.erase(),
                keep_copy_button.erase(),
                keep_mirror_button.erase(),
            ],
            2.0,
        );
        ws.insert_node(Self {
            target,
            size,
            copy_button,
            mirror_button,
            keep_copy_button,
            keep_mirror_button,
            column,
        })
    }
}

#[utils::dynamic_node(skip)]
impl Node for PlacementCommands {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "Placement Commands".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        let drawn = ctx.draw_workspace_node(self.column.erase(), constraints);

        let ws = ctx.node.workspace;
        // Taken, so a command fires once: these rows stop being drawn the
        // moment the menu closes, and a plain read would repeat the last click.
        let taken = |button: NodeUid<Button>| {
            ws.send_request(button.erase(), TakeClicked)
                .unwrap_or(false)
        };
        let root = ws.root();

        if taken(self.copy_button) {
            let copy = ws.deep_clone(self.target);
            ws.submit_action(
                root,
                "Copied node onto the canvas",
                PlaceOnCanvas {
                    node: copy,
                    size: self.size,
                },
            );
        } else if taken(self.mirror_button) {
            let mirror = ws.insert_node_dyn(Arc::new(Mirror::new(self.target)));
            ws.submit_action(
                root,
                "Mirrored node onto the canvas",
                PlaceOnCanvas {
                    node: mirror,
                    size: self.size,
                },
            );
        } else if taken(self.keep_copy_button) {
            ws.submit_action(
                root,
                "Kept a copy in the backpack",
                AddToBackpack {
                    node: self.target,
                    size: self.size,
                    mirror: false,
                },
            );
        } else if taken(self.keep_mirror_button) {
            ws.submit_action(
                root,
                "Kept a mirror in the backpack",
                AddToBackpack {
                    node: self.target,
                    size: self.size,
                    mirror: true,
                },
            );
        }

        drawn.unwrap_or(DrawResult::Complete { region: None })
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.column.erase());
        for button in [
            self.copy_button,
            self.mirror_button,
            self.keep_copy_button,
            self.keep_mirror_button,
        ] {
            ctx.workspace.delete_node(button.erase());
        }
    }
}

defhandlers! { PlacementCommands {} }

/// The inspector's default menu.
#[utils::portable]
pub struct InspectorMenu {
    /// The node this menu describes.
    #[uid_ref]
    target: NodeUid,
    /// The target's own commands, if it has any.
    extra: Option<NodeUid>,
    column: NodeUid<VerticalLayout>,
}

impl InspectorMenu {
    fn build(ws: &Workspace, target: NodeUid, extra: Option<NodeUid>) -> NodeUid<InspectorMenu> {
        let target_ctx = NodeContext {
            id: target,
            workspace: ws,
        };
        let ty_label = ws
            .get_node(target)
            .map(|t| t.type_name(target_ctx))
            .map(Label::new);

        // A name, and whatever the target offers. Commands are the target's to decide.
        let rows = [
            ty_label.map(|l| LayoutChild::Node(Arc::new(l))),
            extra.map(LayoutChild::Id),
        ];
        let column = ws.insert_node(VerticalLayout::new(
            rows.into_iter().flatten().collect(),
            2.0,
        ));

        ws.insert_node(Self {
            target,
            extra,
            column,
        })
    }
}

#[utils::dynamic_node(skip)]
impl Node for InspectorMenu {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "An Inspector Menu".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        ctx.draw_workspace_node(self.column.erase(), constraints)
            .unwrap_or(DrawResult::Complete { region: None })
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.column.erase());
        if let Some(extra) = self.extra {
            ctx.workspace.delete_node(extra);
        }
    }
}

defhandlers! { InspectorMenu {} }

defhandlers! { Inspector {
    actions: [
        // Compose the target's inspector, replacing any menu already open.
        OpenInspector { node: NodeUid, size: Vector } => (this, a, ctx) {
            if let Some(previous) = this.inspector.take() {
                ctx.workspace.delete_node(previous);
            }
            // The target answers for itself, so its context — not the halo's.
            let target_ctx = NodeContext { id: a.node, workspace: ctx.workspace };
            let extra = ctx
                .workspace
                .get_node(a.node)
                .and_then(|node| node.build_inspector(target_ctx));
            this.inspector = Some(
                InspectorMenu::build(ctx.workspace, a.node, extra).erase(),
            );
            this.inspected = Some(a.node);
        },
        CloseInspector => (this, _a, ctx) {
            if let Some(previous) = this.inspector.take() {
                ctx.workspace.delete_node(previous);
            }
            this.inspected = None;
        },
    ],
    requests: [
        // Whether a menu is currently up.
        InspectorOpen => (this, _q): bool { this.inspector.is_some() },
    ],
}}
