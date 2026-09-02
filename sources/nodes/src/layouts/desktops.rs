use dex_core::prelude::*;
use egui::{Color32, LayerId};
use utils::Transient;

use crate::{
    composites::button::Button,
    layouts::{
        canvas::{
            self,
            layout::{AdoptCanvasNode, Canvas, CanvasChildren, Layer},
            nodes::{CanvasItemBounds, CanvasNode, CanvasNodeChild},
            sidebar::CanvasSidebar,
        },
        child::LayoutChild,
        horizontal::HorizontalLayout,
        horizontal_dnd::{AddChild, Children, HorizontalDnD, RemoveChild, Reorder},
        inspector::Inspector,
        mirror::Mirror,
        vertical::VerticalLayout,
    },
    primitives::{
        interaction::{
            InteractionBox, TakeClicked, WasClicked, WasDoubleClicked, WasDragged, WasHovered,
        },
        shapes::Rect,
        text::{GetText, IsInteractive, Label, LabelEditable, SetInteractive},
    },
};

/**
   The workspace root.

   This owns several [`Canvas`] desktops, with one active at a time. It draws a
   sidebar (adds items to the active canvas), a drag-and-drop tab bar, and the
   active canvas.
*/
#[utils::dynamic_type]
#[utils::portable]
pub struct Desktops {
    tab_bar: NodeUid<HorizontalDnD>,
    /// Which desktop is showing.
    #[uid_ref]
    /// The canvas on display.
    active: NodeUid<Canvas>,
    sidebar: NodeUid<CanvasSidebar>,
    add_button: NodeUid<Button>,
    divider: NodeUid<InteractionBox>,

    sidebar_width: f32,
    pending_sidebar_width: Transient<f32>,

    /// A stack of surfaces temporarily shown fullscreen in place of the tabs + active canvas.
    #[dynamic(skip)]
    #[uid_ref]
    override_stack: Vec<NodeUid>,
    close_override_button: NodeUid<Button>,

    /// Whether the sidebar and the tab row are folded away.
    sidebar_collapsed: bool,
    tabs_collapsed: bool,
    /// The buttons that fold each away, and the ones that bring them back.
    collapse_sidebar_button: NodeUid<Button>,
    reveal_sidebar_button: NodeUid<Button>,
    collapse_tabs_button: NodeUid<Button>,
    reveal_tabs_button: NodeUid<Button>,

    /// The single inspector, drawn last so its handle sits over everything.
    inspector: NodeUid<Inspector>,
}

#[utils::dynamic_methods]
impl Desktops {
    /// Build a fresh workspace with a [`Desktops`] instance as its root.
    #[dynamic(skip)]
    pub fn new_workspace() -> Workspace {
        let mut ws = Workspace::new_empty();
        let root = Desktops::build(ws.action_handle());
        // Drain the queued inserts so the tree is live before the first frame.
        ws.process_pending();
        ws.set_root(root.erase());
        ws
    }

    /// Build the desktops root (and its whole subtree) into `ws`.
    pub fn build(ws: WorkspaceActionHandle) -> NodeUid<Desktops> {
        let id = NodeUid::<Desktops>::mint();

        let canvas = Canvas::build(ws.clone());
        let tab = DesktopTabView::build(ws.clone(), canvas, id, "Canvas 1".to_owned());
        let tab_bar = HorizontalDnD::build(ws.clone(), vec![tab.erase()], TAB_SPACING);
        let sidebar = CanvasSidebar::build(ws.clone(), id);
        let add_button = Button::build(ws.clone(), Label::new("+".to_owned()));
        let divider = ws.insert_node(InteractionBox::sensing(true, false, true));
        let close_override_button = Button::build(ws.clone(), Label::new("← Close".to_owned()));
        let chrome = |glyph: &str| {
            Button::build_with(ws.clone(), Label::new(glyph.to_owned()), |b| {
                b.label.font = Font::proportional(12.0);
                b.label.color = Color::gray(110);
                b.padding = 2.0;
                b.corner_radius = 3.0;
                b.fill_color = Color::WHITE;
                b.border = Stroke::new(1.0, Color::gray(205));
            })
        };
        let collapse_sidebar_button = chrome("<");
        let reveal_sidebar_button = chrome(">");
        let collapse_tabs_button = chrome("^");
        let reveal_tabs_button = chrome("v");
        let inspector = ws.insert_node(Inspector::new());

        ws.insert_node_at(
            id,
            Desktops {
                tab_bar,
                active: canvas,
                sidebar,
                add_button,
                divider,
                close_override_button,
                inspector,
                sidebar_width: 200.0,
                pending_sidebar_width: Transient::default(),
                override_stack: Vec::new(),
                sidebar_collapsed: false,
                tabs_collapsed: false,
                collapse_sidebar_button,
                reveal_sidebar_button,
                collapse_tabs_button,
                reveal_tabs_button,
            },
        );
        id
    }
}

const DIVIDER_W: f32 = 6.0;
/// How close the pointer must come to a folded panel's edge to be offered a way back.
const REVEAL_REACH: f32 = 48.0;
/// The little square a collapse or reveal button is drawn into.
const CHROME_SIZE: f32 = 16.0;
const SIDEBAR_MIN: f32 = 120.0;
const SIDEBAR_MAX: f32 = 500.0;
const TAB_BAR_H: f32 = 42.0;
const TAB_SPACING: f32 = 6.0;

impl Desktops {
    /// Build a fresh canvas and its tab into the workspace, returning the canvas.
    fn open_canvas(&self, ctx: NodeContext, name: String) -> NodeUid<Canvas> {
        let canvas = Canvas::build(ctx.workspace.action_handle());
        self.add_tab_for(ctx, canvas, name);
        canvas
    }

    /// Give an existing canvas a tab on this root.
    fn add_tab_for(&self, ctx: NodeContext, canvas: NodeUid<Canvas>, name: String) {
        let tab = DesktopTabView::build(ctx.workspace.action_handle(), canvas, ctx.id.cast(), name);
        ctx.workspace
            .submit_action(self.tab_bar, "Add tab", AddChild { child: tab.erase() });
    }
}

#[utils::dynamic_node]
impl Node for Desktops {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "Desktops".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let avail_w = ctx.constraints.x.map(|a| a.provided_value()).unwrap_or(0.0);
        let avail_h = ctx.constraints.y.map(|a| a.provided_value()).unwrap_or(0.0);
        let origin = ctx.constraints.pos;

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

        let pointer: Option<ScreenPos> = ctx.ui.input(|i| i.pointer.latest_pos().map(Into::into));

        // Left and right step through the tabs.
        if self.override_stack.is_empty() && ctx.ui.memory(|m| m.focused()).is_none() {
            let step = ctx.ui.input_mut(|i| {
                let left = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft);
                let right = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight);
                match (left, right) {
                    (true, false) => -1,
                    (false, true) => 1,
                    _ => 0,
                }
            });
            if step != 0 {
                ctx.submit_action_for_self::<Self, _>(StepTab { by: step }, "Stepped tabs");
            }
        }

        // Draw sidebar ----------------------------------------
        let pending = *self.pending_sidebar_width.val();
        // A folded sidebar takes no width at all; the divider goes with it.
        let sidebar_w = if self.sidebar_collapsed {
            0.0
        } else {
            pending
                .unwrap_or(self.sidebar_width)
                .clamp(SIDEBAR_MIN, SIDEBAR_MAX)
        };
        let divider_w = if self.sidebar_collapsed {
            0.0
        } else {
            DIVIDER_W
        };

        let divider_x = origin.x + sidebar_w;
        let right_x = divider_x + divider_w;
        let right_w = (avail_w - sidebar_w - divider_w).max(0.0);
        let right_origin = ScreenPos {
            x: right_x,
            y: origin.y,
        };

        if !self.sidebar_collapsed {
            ctx.draw_workspace_node(
                self.sidebar.erase(),
                DrawConstraints {
                    pos: origin,
                    x: Some(AxisConstraint::Exactly(sidebar_w)),
                    y: Some(AxisConstraint::Exactly(avail_h)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
            );
        }

        // A folded tab row leaves the content the whole height.
        let tab_bar_h = if self.tabs_collapsed { 0.0 } else { TAB_BAR_H };
        let content_pos = ScreenPos {
            x: right_x,
            y: origin.y + tab_bar_h,
        };
        let content_constraints = DrawConstraints {
            pos: content_pos,
            x: Some(AxisConstraint::Exactly(right_w)),
            y: Some(AxisConstraint::Exactly((avail_h - tab_bar_h).max(0.0))),
            wrap: WrapConstraints::NotAllowed,
            should_clip: true,
        };

        if let Some(&opened) = self.override_stack.last() {
            // An override is open; draw a close button in the tab row and the override filling the content area.
            ctx.draw_workspace_node(
                self.close_override_button.erase(),
                DrawConstraints {
                    pos: right_origin
                        + Vector {
                            x: TAB_SPACING,
                            y: TAB_SPACING,
                        },
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
                .send_request(self.close_override_button.erase(), WasClicked)
                .unwrap_or(false)
            {
                ctx.submit_action_for_self::<Self, _>(PopOverride, "Close override");
            }
            ctx.draw_workspace_node(opened, content_constraints);
        } else if self.tabs_collapsed {
            ctx.draw_workspace_node(self.active.erase(), content_constraints);
        } else {
            // Tab bar, then the add-canvas button, laid out in a row.
            let layout = HorizontalLayout {
                children: vec![
                    LayoutChild::from(self.tab_bar),
                    LayoutChild::from(self.add_button),
                ],
                spacing: TAB_SPACING,
                allow_wrap: false,
            };
            ctx.draw_node(
                &layout,
                DrawConstraints {
                    pos: right_origin
                        + Vector {
                            x: TAB_SPACING,
                            y: TAB_SPACING,
                        },
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

            ctx.draw_workspace_node(self.active.erase(), content_constraints);
        }

        // Draw sidebar splitter ----------------------------------------
        // Only while there is a sidebar to size: an edge with nothing on one
        // side of it is not a handle.
        if !self.sidebar_collapsed {
            ctx.draw_workspace_node(
                self.divider.erase(),
                DrawConstraints {
                    pos: ScreenPos {
                        x: divider_x,
                        y: origin.y,
                    },
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
        }

        // The rule under the tab row, matching the sidebar's own edge: the two
        // panels are bounded the same way, so they read as the same kind of
        // thing.
        let tabs_line_y = origin.y + tab_bar_h;
        if !self.tabs_collapsed {
            ctx.ui.painter().rect_filled(
                ScreenRegion::from_min_size(
                    ScreenPos {
                        x: right_origin.x,
                        y: tabs_line_y - 0.5,
                    },
                    Vector { x: right_w, y: 1.0 },
                )
                .into(),
                0.0,
                Color32::from_gray(220),
            );
        }

        // The fold controls, each sitting on the line it folds away.
        let near = |edge: f32, vertical: bool| {
            pointer.is_some_and(|p| {
                let along = if vertical { p.x } else { p.y };
                (along - edge).abs() <= REVEAL_REACH
            })
        };
        // Centred on the line, then pushed back inside the window: a control
        // half off the edge is half a control.
        let chrome = |ctx: &mut DrawContext, button: NodeUid<Button>, centre: ScreenPos| {
            let half = CHROME_SIZE * 0.5;
            let at = ScreenPos {
                x: (centre.x - half).clamp(origin.x, origin.x + avail_w - CHROME_SIZE),
                y: (centre.y - half).clamp(origin.y, origin.y + avail_h - CHROME_SIZE),
            };
            ctx.draw_workspace_node(
                button.erase(),
                DrawConstraints {
                    pos: at,
                    x: Some(AxisConstraint::Exactly(CHROME_SIZE)),
                    y: Some(AxisConstraint::Exactly(CHROME_SIZE)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: false,
                },
            );
        };

        let (sidebar_button, tabs_button) = (
            if self.sidebar_collapsed {
                self.reveal_sidebar_button
            } else {
                self.collapse_sidebar_button
            },
            if self.tabs_collapsed {
                self.reveal_tabs_button
            } else {
                self.collapse_tabs_button
            },
        );

        // Halfway down the sidebar's edge.
        if !self.sidebar_collapsed || near(origin.x, true) {
            chrome(
                &mut ctx,
                sidebar_button,
                ScreenPos {
                    x: divider_x + divider_w * 0.5,
                    y: origin.y + avail_h * 0.5,
                },
            );
        }
        // Halfway along the tab row's.
        if !self.tabs_collapsed || near(origin.y, false) {
            chrome(
                &mut ctx,
                tabs_button,
                ScreenPos {
                    x: right_origin.x + right_w * 0.5,
                    y: tabs_line_y,
                },
            );
        }

        // Taken, so a click fires once and once only.
        let clicked = |button: NodeUid<Button>| {
            ctx.node
                .workspace
                .send_request(button.erase(), TakeClicked)
                .unwrap_or(false)
        };
        if clicked(sidebar_button) {
            ctx.submit_action_for_self::<Self, _>(ToggleSidebar, "Folded the sidebar");
        }
        if clicked(tabs_button) {
            ctx.submit_action_for_self::<Self, _>(ToggleTabBar, "Folded the tab row");
        }

        // Last, and unclipped.
        ctx.draw_workspace_node(
            self.inspector.erase(),
            DrawConstraints {
                pos: origin,
                x: Some(AxisConstraint::Exactly(avail_w)),
                y: Some(AxisConstraint::Exactly(avail_h)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );

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
        // Unhandled actions/requests fall through to the open override or the active canvas.
        self.override_stack
            .last()
            .copied()
            .or(Some(self.active.erase()))
    }
}

defhandlers! { Desktops {
    actions: [
        AddCanvas => (this, _a, ctx) {
            this.active = this.open_canvas(ctx, "Unnamed desktop".to_owned());
        },
        SetActive { canvas: NodeUid<Canvas> } => (this, s) {
            this.active = s.canvas;
        },

        // Close a canvas and its tab.
        CloseCanvas { tab: NodeUid } => (this, s, ctx) {
            let ws = ctx.workspace;
            let tabs = ws.send_request(this.tab_bar, Children).unwrap_or_default();
            let closing = ws.send_request(s.tab.cast::<DesktopTabView>(), TabCanvas);

            if closing == Some(this.active) {
                // Prefer the tab to the left, as a browser does; fall back to the one on the right.
                let neighbour = tabs
                    .iter()
                    .position(|t| *t == s.tab)
                    .and_then(|i| {
                        // `i - 1` underflows to `None` at the first tab.
                        tabs.get(i.wrapping_sub(1)).or_else(|| tabs.get(i + 1))
                    })
                    .and_then(|t| ws.send_request(t.cast::<DesktopTabView>(), TabCanvas));
                // Closing the last desktop opens an empty one in its place.
                this.active = match neighbour {
                    Some(canvas) => canvas,
                    None => this.open_canvas(ctx, "Unnamed desktop".to_owned()),
                };
            }

            ws.submit_action(this.tab_bar, "Removed tab", RemoveChild { child: s.tab });
            ws.delete_node(s.tab);
        },
        // Duplicate a desktop: the canvas and everything on it, deep cloned.
        CloneCanvas { tab: NodeUid } => (this, s, ctx) {
            let ws = ctx.workspace;
            if let Some(source) = ws.send_request(s.tab.cast::<DesktopTabView>(), TabCanvas) {
                let copy = ws.deep_clone(source.erase()).cast::<Canvas>();
                let name = ws
                    .send_request(s.tab.cast::<DesktopTabView>(), TabName)
                    .unwrap_or_else(|| "Unnamed desktop".to_owned());
                this.add_tab_for(ctx, copy, format!("{name} copy"));
                this.active = copy;
            }
        },
        // A new canvas whose items mirror this one's.
        MirrorCanvas { tab: NodeUid } => (this, s, ctx) {
            let ws = ctx.workspace;
            if let Some(source) = ws.send_request(s.tab.cast::<DesktopTabView>(), TabCanvas) {
                let mirrored = Canvas::build(ws.action_handle());
                for item in ws.send_request(source, CanvasChildren).unwrap_or_default() {
                    if let Some(child) = ws.send_request(item, CanvasNodeChild)
                        && let Some(bounds) = ws.send_request(item, CanvasItemBounds)
                    {
                        let mirror = ws.insert_node_dyn(Arc::new(Mirror::new(child)));
                        let framed = CanvasNode::build(
                            ws.action_handle(),
                            mirror,
                            bounds.min.to_vector(),
                            bounds.size(),
                        );
                        ws.submit_action(
                            mirrored,
                            "Mirrored canvas item",
                            AdoptCanvasNode {
                                node: framed.erase(),
                                layer: Layer::Midground,
                            },
                        );
                    }
                }
                let name = ws
                    .send_request(s.tab.cast::<DesktopTabView>(), TabName)
                    .unwrap_or_else(|| "Unnamed desktop".to_owned());
                this.add_tab_for(ctx, mirrored, format!("{name} mirror"));
                this.active = mirrored;
            }
        },
        // Put a tab at `to` in the bar, clamped to the ends.
        MoveTab { tab: NodeUid, to: usize } => (this, s, ctx) {
            let ws = ctx.workspace;
            let tabs = ws.send_request(this.tab_bar, Children).unwrap_or_default();
            if let Some(from) = tabs.iter().position(|t| *t == s.tab) {
                let to = s.to.min(tabs.len().saturating_sub(1));
                if from != to {
                    ws.submit_action(this.tab_bar, "Reordered tabs", Reorder { from, to });
                }
            }
        },
        SetSidebarWidth { width: f32 } => (this, s) {
            this.sidebar_width = s.width;
        },
        // Keep `node` in the sidebar's backpack.
        AddToBackpack { node: NodeUid, size: Vector, mirror: bool } => (this, s, ctx) {
            ctx.workspace.submit_action(
                this.sidebar,
                "Added to the backpack",
                canvas::sidebar::StoreInBackpack {
                    node: s.node,
                    size: s.size,
                    mirror: s.mirror,
                },
            );
        },
        // Open `node` fullscreen in place of the tabs/canvas.
        // Fold the sidebar away, or bring it back.
        ToggleSidebar => (this, _a) { this.sidebar_collapsed = !this.sidebar_collapsed; },
        // The same for the row of canvas tabs.
        ToggleTabBar => (this, _a) { this.tabs_collapsed = !this.tabs_collapsed; },
        // Show the tab `by` places along from the open one.
        StepTab { by: isize } => (this, s, ctx) {
            let ws = ctx.workspace;
            let tabs = ws.send_request(this.tab_bar, Children).unwrap_or_default();
            let canvases: Vec<NodeUid<Canvas>> = tabs
                .iter()
                .filter_map(|t| ws.send_request(t.cast::<DesktopTabView>(), TabCanvas))
                .collect();
            let at = canvases.iter().position(|c| *c == this.active);
            if let Some(at) = at.filter(|_| canvases.len() > 1) {
                let count = canvases.len() as isize;
                let next = (at as isize + s.by).rem_euclid(count) as usize;
                this.active = canvases[next];
            }
        },
        PushOverride { node: NodeUid } => (this, s) {
            this.override_stack.push(s.node);
        },
        // Return to the surface beneath the current override.
        PopOverride => (this, _a) {
            this.override_stack.pop();
        },
    ],
    requests: [
        ActiveCanvas => (this, _q): NodeUid<Canvas> { this.active },
        // The open tabs, in display order.
        Tabs => (this, _q, ctx): Vec<NodeUid> {
            ctx.workspace.send_request(this.tab_bar, Children).unwrap_or_default()
        },
        PythonPrelude => (this, _q, ctx): String {
            ctx
                .workspace
                .send_request(this.sidebar, canvas::sidebar::SidebarPythonPrelude {})
                .expect("Canvas sidebar should exist and understand SidebarPythonPrelude request")
        },
    ],
    extern_requests: [
        // The inspector belongs to the root, so ask the root about it.
        crate::layouts::inspector::InspectorOpen => (this, s, ctx): bool {
            ctx.workspace.send_request(this.inspector, s).unwrap_or(false)
        },
    ],
}}

/// Display node for a single tab (labelled by canvas name).
#[derive(Copy)]
#[utils::portable]
pub struct DesktopTabView {
    canvas: NodeUid<Canvas>,
    name: NodeUid<LabelEditable>,
    /// A back-reference to the root.
    #[uid_ref]
    parent: NodeUid<Desktops>,
    /// Click/double-click sensor over the whole tab.
    sensor: NodeUid<InteractionBox>,
    delete_button: NodeUid<Button>,
}

impl DesktopTabView {
    /// Build a tab (its editable name + click sensor) into `ws`.
    fn build(
        ws: WorkspaceActionHandle,
        canvas: NodeUid<Canvas>,
        parent: NodeUid<Desktops>,
        name_text: String,
    ) -> NodeUid<DesktopTabView> {
        let name = ws.insert_node(LabelEditable::click_to_edit(name_text));
        let sensor = ws.insert_node(InteractionBox::sensing(false, true, false));
        let delete_button = Button::build_with(ws.clone(), Label::new("×".to_owned()), |b| {
            b.padding = 2.0;
            b.corner_radius = 3.0;
            b.border = Stroke::NONE;
        });
        ws.insert_node(Self {
            canvas,
            name,
            parent,
            sensor,
            delete_button,
        })
    }
}

/// What a desktop tab offers the inspector.
#[utils::portable]
pub struct TabInspector {
    /// The tab these commands act on.
    #[uid_ref]
    tab: NodeUid<DesktopTabView>,
    clone_button: NodeUid<Button>,
    mirror_button: NodeUid<Button>,
    delete_button: NodeUid<Button>,
    front_button: NodeUid<Button>,
    back_button: NodeUid<Button>,
    left_button: NodeUid<Button>,
    right_button: NodeUid<Button>,
    column: NodeUid<VerticalLayout>,
}

impl TabInspector {
    fn build(ctx: NodeContext, tab: NodeUid<DesktopTabView>) -> NodeUid<TabInspector> {
        let ws = ctx.workspace.action_handle();
        let command = |label: &str| Button::build(ws.clone(), Label::new(label.to_owned()));
        let clone_button = command("Clone desktop");
        let mirror_button = command("Mirror desktop");
        let delete_button = command("Delete");
        let front_button = command("Move to front");
        let back_button = command("Move to back");
        let left_button = command("Move left");
        let right_button = command("Move right");

        let column = VerticalLayout::build(
            ws.clone(),
            vec![
                clone_button.erase(),
                mirror_button.erase(),
                delete_button.erase(),
                front_button.erase(),
                back_button.erase(),
                left_button.erase(),
                right_button.erase(),
            ],
            2.0,
        );
        ws.insert_node(Self {
            tab,
            clone_button,
            mirror_button,
            delete_button,
            front_button,
            back_button,
            left_button,
            right_button,
            column,
        })
    }
}

#[utils::dynamic_node(skip)]
impl Node for TabInspector {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "Desktop Menu".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        let drawn = ctx.draw_workspace_node(self.column.erase(), constraints);

        let ws = ctx.node.workspace;
        let taken = |button: NodeUid<Button>| {
            ws.send_request(button.erase(), TakeClicked)
                .unwrap_or(false)
        };
        let root = ws.root();
        let tab = self.tab.erase();

        // The tab's place in the bar, for the reordering commands.
        let tabs = ws
            .send_request(root.cast::<Desktops>(), Tabs)
            .unwrap_or_default();
        let at = tabs.iter().position(|t| *t == tab);

        if taken(self.clone_button) {
            ws.submit_action(root, "Cloned desktop", CloneCanvas { tab });
        } else if taken(self.mirror_button) {
            ws.submit_action(root, "Mirrored desktop", MirrorCanvas { tab });
        } else if taken(self.delete_button) {
            ws.submit_action(root, "Closed desktop", CloseCanvas { tab });
        } else if taken(self.front_button) {
            ws.submit_action(root, "Moved desktop to front", MoveTab { tab, to: 0 });
        } else if taken(self.back_button) {
            ws.submit_action(
                root,
                "Moved desktop to back",
                MoveTab {
                    tab,
                    to: tabs.len().saturating_sub(1),
                },
            );
        } else if taken(self.left_button) {
            if let Some(at) = at.filter(|at| *at > 0) {
                ws.submit_action(root, "Moved desktop left", MoveTab { tab, to: at - 1 });
            }
        } else if taken(self.right_button)
            && let Some(at) = at
        {
            ws.submit_action(root, "Moved desktop right", MoveTab { tab, to: at + 1 });
        }

        drawn.unwrap_or(DrawResult::Complete { region: None })
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.column.erase());
        for button in [
            self.clone_button,
            self.mirror_button,
            self.delete_button,
            self.front_button,
            self.back_button,
            self.left_button,
            self.right_button,
        ] {
            ctx.workspace.delete_node(button.erase());
        }
    }
}

defhandlers! { TabInspector {} }

#[utils::dynamic_node(skip)]
impl Node for DesktopTabView {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Desktop Tab".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const PAD_X: f32 = 10.0;
        const PAD_Y: f32 = 5.0;
        /// Space between the name and the close button.
        const GAP: f32 = 6.0;

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
        let origin = ctx.constraints.pos;

        let editing = ctx
            .node
            .workspace
            .send_request(self.name, IsInteractive)
            .unwrap_or(false);
        let active =
            ctx.node.workspace.send_request(self.parent, ActiveCanvas) == Some(self.canvas);

        // The editable name, inset by the padding.
        let name_res = ctx.draw_workspace_node(
            self.name.erase(),
            DrawConstraints {
                pos: origin + Vector { x: PAD_X, y: PAD_Y },
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

        // The close button follows the name; drawn unconstrained, so it reports
        // the natural size the tab is then sized around.
        let button_res = ctx.draw_workspace_node(
            self.delete_button.erase(),
            DrawConstraints {
                pos: origin
                    + Vector {
                        x: PAD_X + name_size.x + GAP,
                        y: PAD_Y,
                    },
                x: None,
                y: None,
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );
        let button_size = button_res
            .and_then(|r| r.region())
            .map(|r| r.size())
            .unwrap_or(Vector { x: 14.0, y: 14.0 });

        // Everything left of the close button is the tab's own click target.
        let name_area_w = PAD_X + name_size.x + GAP;
        let tab_size = Vector {
            x: name_area_w + button_size.x + PAD_X,
            y: name_size.y.max(button_size.y) + 2.0 * PAD_Y,
        };

        // Outline; the active tab gets a stronger accent.
        let border = if active {
            Stroke::new(1.5, Color::rgb(70, 130, 180))
        } else {
            Stroke::new(1.0, Color::gray(210))
        };
        let outline = Rect {
            size: tab_size,
            corner_radius: 4.0,
            fill_color: Color::TRANSPARENT,
            border,
            stroke_kind: StrokeKind::Middle,
        };
        outline.paint(ctx.ui.painter(), origin);

        // Click/double-click sensor over the whole tab when not editing.
        if !editing {
            ctx.draw_workspace_node(
                self.sensor.erase(),
                DrawConstraints {
                    pos: origin,
                    // Stops short of the close button.
                    x: Some(AxisConstraint::Exactly(name_area_w)),
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

        if ctx
            .node
            .workspace
            .send_request(self.delete_button.erase(), WasClicked)
            .unwrap_or(false)
        {
            ctx.node.workspace.submit_action(
                self.parent,
                "Closed canvas",
                CloseCanvas { tab: ctx.node.id },
            );
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, tab_size)),
        }
    }

    fn build_inspector(&self, ctx: NodeContext) -> Option<NodeUid> {
        Some(TabInspector::build(ctx, ctx.id.cast()).erase())
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.name.erase());
        ctx.workspace.delete_node(self.sensor.erase());
        ctx.workspace.delete_node(self.delete_button.erase());
        ctx.workspace.delete_node(self.canvas.erase());
    }
}

defhandlers! { DesktopTabView {
    requests: [
        // The canvas this tab stands for.
        TabCanvas => (this, _q): NodeUid<Canvas> { this.canvas },
        // The tab's label, for naming a copy after it.
        TabName => (this, _q, ctx): String {
            ctx.workspace.send_request(this.name, GetText).unwrap_or_default()
        },
    ],
}}
