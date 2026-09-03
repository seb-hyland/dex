use dex_core::prelude::*;
use dex_core::theme;
use egui::{Id, Popup, PopupCloseBehavior, PopupKind, RectAlign, Sense};
use utils::Transient;

use crate::{
    composites::button::Button,
    layouts::{
        LayoutChild,
        canvas::layout::{BringCanvasItemToFront, PlaceOnCanvas, SendCanvasItemToBack},
        desktops::AddToBackpack,
        mirror::Mirror,
        vertical::VerticalLayout,
    },
    primitives::{interaction::TakeClicked, shapes::Rect, text::Label},
    scripting::DataflowOutput,
};

/// How far up and left of its node the lens sits.
const HANDLE_OFFSET: f32 = 8.0;
const HANDLE_SIZE: Vector = Vector { x: 14.0, y: 22.0 };
/// The menu's width. Fixed, as the popup sizes itself to its contents.
const MENU_WIDTH: f32 = 180.0;
/// How far the pointer may stray from the menu before it closes.
const MENU_SLACK: f32 = 36.0;
/// Once the pointer is this near the lens, it holds its target outright.
const LENS_GRACE: f32 = 24.0;
/// How long a new node must hold the pointer before the lens moves to it.
const LENS_DWELL: f64 = 0.18;
/// A stable egui id: there is only ever one handle.
const HANDLE_ID: &str = "dex_inspector_handle";

/**
    A row in an inspector menu.

    Menu rows carry no outline of their own: a column of eight bordered
    buttons reads as eight separate controls rather than one list. The row
    only shows a ground when the pointer is on it, which is also what tells
    you which one you are about to press.
*/
pub fn menu_button(ws: WorkspaceActionHandle, label: &str) -> NodeUid<Button> {
    Button::build_with(ws, Label::new(label.to_owned()), |b| {
        b.padding = theme::SPACE_SM;
        b.padding_x = theme::SPACE_SM;
        b.corner_radius = theme::RADIUS_SM;
        b.border = Stroke::NONE;
        b.fill_width = true;
    })
}

/// Where the lens sits, and what is queued to take it over.
#[derive(Clone)]
pub struct HandleState {
    pub target: NodeUid,
    pub region: ScreenRegion,
    /// What the pointer has wandered onto since, and when it got there.
    pub candidate: Option<NodeUid>,
    pub since: f64,
}

/// Where the lens sits for a target that drew into `region`.
fn lens_region(region: ScreenRegion) -> ScreenRegion {
    ScreenRegion::from_min_size(
        ScreenPos {
            x: region.min.x - HANDLE_OFFSET,
            y: region.min.y - HANDLE_OFFSET,
        },
        HANDLE_SIZE,
    )
}

/// Which node the lens should be offering, given what is under the pointer.
fn settle(
    held: Option<HandleState>,
    found: Option<InspectTarget>,
    pointer: Option<ScreenPos>,
    now: f64,
) -> Option<HandleState> {
    let fresh = |target: InspectTarget| HandleState {
        target: target.node,
        region: target.region,
        candidate: None,
        since: now,
    };
    let Some(held) = held else {
        // Nothing showing: the first thing under the pointer takes it.
        return found.map(fresh);
    };

    // On the lens, or as good as. Whatever else the pointer is over, it is
    // there to click this.
    if pointer.is_some_and(|p| lens_region(held.region).distance_to(p) <= LENS_GRACE) {
        return Some(HandleState {
            candidate: None,
            since: now,
            ..held
        });
    }

    // Still on the same node: follow it, in case it moved or resized.
    if let Some(target) = &found
        && target.node == held.target
    {
        return Some(fresh(target.clone()));
    }

    // Somewhere else — or nowhere, which is a candidate of its own, so that
    // leaving the drawing puts the lens away rather than stranding it.
    let candidate = found.as_ref().map(|t| t.node);
    if held.candidate == candidate && now - held.since >= LENS_DWELL {
        return found.map(fresh);
    }
    Some(HandleState {
        candidate,
        since: if held.candidate == candidate {
            held.since
        } else {
            now
        },
        ..held
    })
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

        // Reaching for the lens means leaving the thing that offered it, so the
        // lens waits for a new node to hold the pointer before it follows.
        let pointer: Option<ScreenPos> = ctx.ui.ctx().pointer_latest_pos().map(Into::into);
        let now = ctx.ui.ctx().input(|i| i.time);
        let settled = settle(
            held.clone().filter(|h| ws.get_node(h.target).is_some()),
            found.clone(),
            pointer,
            now,
        );

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
            _ => settled.as_ref().map(|h| (h.target, h.region)),
        };

        let Some((target, region)) = shown else {
            self.handle.val_mut().take();
            if self.inspector.is_some() {
                ctx.submit_action_for_self::<Self, _>(CloseInspector, "Closed inspector");
            }
            return DrawResult::Complete { region: None };
        };

        // Beside the node, in the margin, like the canvas handle it replaces.
        let handle_region = lens_region(region);
        // The inspector owns its chrome, so it interacts directly.
        let resp = ctx
            .ui
            .interact(handle_region.into(), Id::new(HANDLE_ID), Sense::CLICK);
        let engaged =
            resp.hovered() || resp.is_pointer_button_down_on() || self.inspector.is_some();

        Rect {
            size: HANDLE_SIZE,
            corner_radius: theme::RADIUS_MD,
            fill_color: if engaged {
                theme::SURFACE_SUNKEN
            } else {
                Color::rgba(0, 0, 0, 12)
            },
            border: if engaged {
                theme::border_hover()
            } else {
                Stroke::NONE
            },
            stroke_kind: StrokeKind::Middle,
        }
        .paint(ctx.ui.painter(), handle_region.min);

        // A magnifying glass: this inspects, it does not move anything.
        let ink = egui::Color32::from(if engaged {
            theme::INK
        } else {
            theme::INK_FAINT
        });
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

        // What the sticky path settled on, if the menu overrode the dwell.
        self.handle.set(match settled {
            Some(state) if state.target == target => state,
            _ => HandleState {
                target,
                region,
                candidate: None,
                since: now,
            },
        });

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
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
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
            // The menu draws into a `Ui` of the popup's.
            let mut menu_ctx = ctx.child(ui, node, constraints);
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

/**
    The placement commands for `target`, if `target` is a node's result.

    A canvas item gets these from [`CanvasNode`](crate::layouts::canvas::nodes::CanvasNode),
    but a transform's output is not a canvas item and so had nothing — there was
    no way to copy or keep what a lambda had just computed. A result is
    recognised the way the dataflow protocol defines one: the node that owns
    `target` reports `target` as its `DataflowOutput`.
*/
fn result_commands(ws: &Workspace, target: NodeUid) -> Option<NodeUid<PlacementCommands>> {
    let owner = owner_of(ws, target)?;
    if ws.send_request(owner, DataflowOutput).flatten() != Some(target) {
        return None;
    }
    // The size it last drew at.
    let size = ws
        .inspectable_rect(target)
        .map(|region| region.size())
        .unwrap_or(DEFAULT_RESULT_SIZE);
    Some(PlacementCommands::build(ws.action_handle(), target, size))
}

/// The node that owns `target`, by the same relation a deep clone follows.
fn owner_of(ws: &Workspace, target: NodeUid) -> Option<NodeUid> {
    ws.live_ids().into_iter().find(|&candidate| {
        let mut owns = false;
        if let Some(node) = ws.get_node(candidate) {
            node.owned_refs(&mut |child| owns |= child == target);
        }
        owns
    })
}

/// What a copied result takes when nothing recorded where it drew.
const DEFAULT_RESULT_SIZE: Vector = Vector { x: 160.0, y: 60.0 };

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
    front_button: Option<NodeUid<Button>>,
    back_button: Option<NodeUid<Button>>,
    /// Opens the target over the whole content area.
    fullscreen_button: NodeUid<Button>,
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
        Self::assemble(ws, target, size, false)
    }

    /// The same commands for a `target` that is a top-level canvas item.
    pub fn build_for_canvas_item(
        ws: WorkspaceActionHandle,
        target: NodeUid,
        size: Vector,
    ) -> NodeUid<PlacementCommands> {
        Self::assemble(ws, target, size, true)
    }
}

impl PlacementCommands {
    fn assemble(
        ws: WorkspaceActionHandle,
        target: NodeUid,
        size: Vector,
        restackable: bool,
    ) -> NodeUid<PlacementCommands> {
        let command = |label: &str| menu_button(ws.clone(), label);
        let copy_button = command("Copy");
        let mirror_button = command("Mirror");
        let keep_copy_button = command("Copy to Backpack");
        let keep_mirror_button = command("Mirror to Backpack");
        let front_button = restackable.then(|| command("Bring to Front"));
        let back_button = restackable.then(|| command("Send to Back"));
        // Offered to everything with these commands, not just to canvas items:
        // a lambda's result has no place on the canvas to be restacked in, and
        // is exactly the thing worth seeing big.
        let fullscreen_button = command("Open Fullscreen");
        let column = VerticalLayout::build(
            ws.clone(),
            [
                Some(copy_button.erase()),
                Some(mirror_button.erase()),
                Some(keep_copy_button.erase()),
                Some(keep_mirror_button.erase()),
                front_button.map(|b| b.erase()),
                back_button.map(|b| b.erase()),
                Some(fullscreen_button.erase()),
            ]
            .into_iter()
            .flatten()
            .collect(),
            2.0,
        );
        ws.insert_node(Self {
            target,
            size,
            copy_button,
            mirror_button,
            keep_copy_button,
            keep_mirror_button,
            front_button,
            back_button,
            fullscreen_button,
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
        } else if self.front_button.is_some_and(&taken) {
            ws.submit_action(
                root,
                "Brought the item to the front",
                BringCanvasItemToFront { node: self.target },
            );
        } else if self.back_button.is_some_and(taken) {
            ws.submit_action(
                root,
                "Sent the item to the back",
                SendCanvasItemToBack { node: self.target },
            );
        } else if taken(self.fullscreen_button) {
            ws.submit_action(
                root.cast::<crate::layouts::desktops::Desktops>(),
                "Opened fullscreen",
                crate::layouts::desktops::PushOverride { node: self.target },
            );
        }

        drawn.unwrap_or(DrawResult::Complete { region: None })
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.column.erase());
        for button in [
            Some(self.copy_button),
            Some(self.mirror_button),
            Some(self.keep_copy_button),
            Some(self.keep_mirror_button),
            self.front_button,
            self.back_button,
            Some(self.fullscreen_button),
        ]
        .into_iter()
        .flatten()
        {
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

        // A name, the placement commands if this is a result, and whatever the
        // target offers. Everything else is the target's to decide.
        let rows = [
            ty_label.map(|l| LayoutChild::Node(Arc::new(l))),
            result_commands(ws, target).map(|c| LayoutChild::Id(c.erase())),
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
        // The node the lens is offering, which is not always the one under the
        // pointer: while the pointer is travelling to the lens, the lens holds.
        LensTarget => (this, _q): Option<NodeUid> {
            this.handle.val().as_ref().map(|held| held.target)
        },
        // Where the lens is drawn, for anything that has to reach it.
        LensRegion => (this, _q): Option<ScreenRegion> {
            this.handle.val().as_ref().map(|held| lens_region(held.region))
        },
    ],
}}
