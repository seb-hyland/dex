//! What the sidebar's backpack keeps, and what a kept thing does when clicked.

use dex_core::prelude::*;
use dex_core::theme;
use utils::Transient;

use crate::{
    composites::button::Button,
    layouts::{
        canvas::{
            layout::PlaceOnCanvas,
            sidebar::{BackpackList, CanvasSidebar},
        },
        horizontal_dnd::RemoveChild,
        mirror::Mirror,
        vertical::VerticalLayout,
    },
    primitives::{
        interaction::{InteractionBox, TakeClicked},
        shapes::Rect,
        text::{IsInteractive, Label, LabelEditable, SetInteractive},
    },
};

/// Space between a row's edge and its name.
const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 4.0;
/// The column the mirror mark sits in. Kept clear on every row, so names line up.
const MARK_W: f32 = 14.0;
/// The side of the mirror mark itself.
const MARK_SIZE: f32 = 9.0;

/// One thing kept in the sidebar's backpack.
#[utils::portable]
pub struct BackpackItem {
    /// A back-reference to the sidebar keeping this.
    #[uid_ref]
    sidebar: NodeUid<CanvasSidebar>,
    /// The detached node this entry stamps copies of. Set for a template entry.
    template: Option<NodeUid>,
    /// The live node a mirror entry reflects. Not owned: it belongs to a canvas.
    #[uid_ref]
    source: Option<NodeUid>,
    /// The size a placement takes.
    size: Vector,

    name: NodeUid<LabelEditable>,
    /// Click sensor over the whole row.
    sensor: NodeUid<InteractionBox>,
    /// When a click landed, while it waits out the double-click that would rename instead.
    pending_click: Transient<f64>,
}

impl BackpackItem {
    /// An entry that stamps out copies of the detached `template`.
    pub fn template(
        ws: WorkspaceActionHandle,
        sidebar: NodeUid<CanvasSidebar>,
        template: NodeUid,
        name: String,
        size: Vector,
    ) -> NodeUid<BackpackItem> {
        Self::build(ws, sidebar, Some(template), None, name, size)
    }

    /// An entry that places mirrors of `source`.
    pub fn mirror(
        ws: WorkspaceActionHandle,
        sidebar: NodeUid<CanvasSidebar>,
        source: NodeUid,
        name: String,
        size: Vector,
    ) -> NodeUid<BackpackItem> {
        Self::build(ws, sidebar, None, Some(source), name, size)
    }

    fn build(
        ws: WorkspaceActionHandle,
        sidebar: NodeUid<CanvasSidebar>,
        template: Option<NodeUid>,
        source: Option<NodeUid>,
        name: String,
        size: Vector,
    ) -> NodeUid<BackpackItem> {
        let mut label = LabelEditable::click_to_edit(name);
        label.font = theme::text();
        let name = ws.insert_node(label);
        let sensor = ws.insert_node(InteractionBox::sensing(false, true, false));
        ws.insert_node(Self {
            sidebar,
            template,
            source,
            size,
            name,
            sensor,
            pending_click: Transient::default(),
        })
    }

    /// Whether this entry places mirrors rather than copies.
    fn is_mirror(&self) -> bool {
        self.source.is_some()
    }

    /// The mark that says a row places mirrors.
    fn paint_mark(&self, ctx: &mut DrawContext, at: ScreenPos, live: bool) {
        let ink = if live {
            Color::rgb(126, 92, 166)
        } else {
            theme::LINE
        };
        let painter = ctx.ui.painter();
        Rect {
            size: Vector {
                x: MARK_SIZE * 0.5,
                y: MARK_SIZE,
            },
            corner_radius: 0.0,
            fill_color: Color::rgba(ink.r, ink.g, ink.b, 90),
            border: Stroke::NONE,
            stroke_kind: StrokeKind::Inside,
        }
        .paint(painter, at);
        Rect {
            size: Vector::splat(MARK_SIZE),
            corner_radius: theme::RADIUS_SM,
            fill_color: Color::TRANSPARENT,
            border: Stroke::new(1.0, ink),
            stroke_kind: StrokeKind::Inside,
        }
        .paint(painter, at);
    }

    /// Whether a mirror entry's source is still around to reflect.
    fn source_is_live(&self, ctx: NodeContext) -> bool {
        self.source
            .is_none_or(|source| ctx.workspace.get_node(source).is_some())
    }

    /// Act on this frame's clicks: one to place, two to rename.
    fn poll_row(&self, ctx: &mut DrawContext) {
        let ws = ctx.node.workspace;
        let now = ctx.ui.input(|i| i.time);
        let grace = ctx
            .ui
            .ctx()
            .options(|o| o.input_options.max_double_click_delay);
        let pending = *self.pending_click.val();

        if ws
            .send_request(self.sensor.erase(), TakeClicked)
            .unwrap_or(false)
        {
            match pending {
                Some(first) if now - first < grace => {
                    // The click that opened this pair never happened.
                    *self.pending_click.val_mut() = None;
                    ctx.submit_action_for_self::<Self, _>(
                        RenameBackpackItem,
                        "Renaming a backpack item",
                    );
                }
                _ => self.pending_click.set(now),
            }
            return;
        }

        if pending.is_some_and(|at| now - at >= grace) {
            *self.pending_click.val_mut() = None;
            ctx.submit_action_for_self::<Self, _>(PlaceFromBackpack, "Placed from the backpack");
        }
    }
}

#[utils::dynamic_node(skip)]
impl Node for BackpackItem {
    fn type_name(&self, _ctx: NodeContext) -> String {
        if self.is_mirror() {
            "A Backpack Mirror".into()
        } else {
            "A Backpack Template".into()
        }
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let ws = ctx.node.workspace;
        let origin = ctx.constraints.pos;
        let avail_w = ctx
            .constraints
            .x
            .map(|a| a.provided_value())
            .filter(|w| w.is_finite())
            .unwrap_or(160.0);
        let editing = ws.send_request(self.name, IsInteractive).unwrap_or(false);
        let live = self.source_is_live(ctx.node);

        // The name, inset past the mark's column so every row's text lines up.
        let text_x = PAD_X + MARK_W;
        let name_size = ctx
            .draw_workspace_node(
                self.name.erase(),
                DrawConstraints {
                    pos: origin
                        + Vector {
                            x: text_x,
                            y: PAD_Y,
                        },
                    x: Some(AxisConstraint::AtMost((avail_w - text_x - PAD_X).max(0.0))),
                    y: None,
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
            )
            .and_then(|r| r.region())
            .map(|r| r.size())
            .unwrap_or(Vector { x: 0.0, y: 18.0 });
        let size = Vector {
            x: avail_w,
            y: name_size.y + 2.0 * PAD_Y,
        };

        // A mirror with nothing left to reflect says so rather than placing a blank.
        if !live {
            let mut gone = Label::new("gone".to_owned());
            gone.font = theme::text_heading();
            gone.color = theme::INK_FAINT;
            ctx.draw_node(
                &gone,
                DrawConstraints {
                    pos: origin
                        + Vector {
                            x: text_x + name_size.x + 6.0,
                            y: PAD_Y + 2.0,
                        },
                    x: Some(AxisConstraint::AtMost(
                        (avail_w - text_x - name_size.x - PAD_X - 6.0).max(0.0),
                    )),
                    y: None,
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
            );
        }

        if self.is_mirror() {
            self.paint_mark(
                &mut ctx,
                origin
                    + Vector {
                        x: PAD_X,
                        y: (size.y - MARK_SIZE) * 0.5,
                    },
                live,
            );
        }

        // Unfilled, so the chrome goes down after the name without covering it.
        Rect {
            size,
            corner_radius: theme::RADIUS_MD,
            fill_color: Color::TRANSPARENT,
            border: Stroke::new(1.0, theme::LINE),
            stroke_kind: StrokeKind::Inside,
        }
        .paint(ctx.ui.painter(), origin);

        // While the name is being edited it owns the row, exactly as a tab's does.
        if !editing {
            ctx.draw_workspace_node(
                self.sensor.erase(),
                DrawConstraints {
                    pos: origin,
                    x: Some(AxisConstraint::Exactly(size.x)),
                    y: Some(AxisConstraint::Exactly(size.y)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: false,
                },
            );
            self.poll_row(&mut ctx);
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, size)),
        }
    }

    fn build_inspector(&self, ctx: NodeContext) -> Option<NodeUid> {
        Some(BackpackItemCommands::build(ctx, ctx.id.cast()).erase())
    }

    fn deref_target(&self) -> Option<NodeUid> {
        // Polling messages fall through to the row's click sensor.
        Some(self.sensor.erase())
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.name.erase());
        ctx.workspace.delete_node(self.sensor.erase());
        // The template is this entry's own; a mirror's source belongs to a canvas.
        if let Some(template) = self.template {
            ctx.workspace.delete_node(template);
        }
    }
}

defhandlers! { BackpackItem {
    actions: [
        // Put one of these on the active canvas.
        PlaceFromBackpack => (this, _a, ctx) {
            let ws = ctx.workspace;
            let placed = match (this.template, this.source) {
                (Some(template), _) => Some(ws.deep_clone(template)),
                (None, Some(source)) => Some(ws.insert_node_dyn(Arc::new(Mirror::new(source)))),
                (None, None) => None,
            };
            if let Some(node) = placed {
                ws.submit_action(
                    ws.root(),
                    "Placed a backpack item",
                    PlaceOnCanvas { node, size: this.size },
                );
            }
        },
        // Put the name into edit mode; it locks itself again on focus loss.
        RenameBackpackItem => (this, _a, ctx) {
            ctx.workspace.submit_action(
                this.name,
                "Renaming a backpack item",
                SetInteractive { on: true },
            );
        },
        // Take this entry out of the backpack for good.
        DiscardFromBackpack => (this, _a, ctx) {
            let ws = ctx.workspace;
            if let Some(list) = ws.send_request(this.sidebar, BackpackList) {
                ws.submit_action(list, "Removed from the backpack", RemoveChild { child: ctx.id });
            }
            ws.delete_node(ctx.id);
        },
    ],
}}

/// What a backpack entry offers the inspector.
#[utils::portable]
pub struct BackpackItemCommands {
    /// The entry these commands act on.
    #[uid_ref]
    item: NodeUid<BackpackItem>,
    remove_button: NodeUid<Button>,
    column: NodeUid<VerticalLayout>,
}

impl BackpackItemCommands {
    fn build(ctx: NodeContext, item: NodeUid<BackpackItem>) -> NodeUid<BackpackItemCommands> {
        let ws = ctx.workspace.action_handle();
        let remove_button = Button::build_with(ws.clone(), Label::new("Remove".to_owned()), |b| {
            b.padding = 4.0;
            b.corner_radius = 3.0;
            b.border = Stroke::NONE;
            b.fill_width = true;
        });
        let column = VerticalLayout::build(ws.clone(), vec![remove_button.erase()], 2.0);
        ws.insert_node(Self {
            item,
            remove_button,
            column,
        })
    }
}

#[utils::dynamic_node(skip)]
impl Node for BackpackItemCommands {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Backpack Item Menu".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        let drawn = ctx.draw_workspace_node(self.column.erase(), constraints);

        let ws = ctx.node.workspace;
        // Taken, so the command fires once.
        if ws
            .send_request(self.remove_button.erase(), TakeClicked)
            .unwrap_or(false)
        {
            ws.submit_action(self.item, "Removed from the backpack", DiscardFromBackpack);
        }

        drawn.unwrap_or(DrawResult::Complete { region: None })
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.column.erase());
        ctx.workspace.delete_node(self.remove_button.erase());
    }
}

defhandlers! { BackpackItemCommands {} }
