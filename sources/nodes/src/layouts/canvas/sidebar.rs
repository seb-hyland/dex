use dex_core::prelude::*;
use utils::Transient;

use crate::{
    composites::{
        button::{Button, SetButtonStyle},
        lambda::{CanvasLambda, Lambda},
    },
    layouts::{
        HorizontalLayout, LayoutChild, ScrollLayout,
        canvas::{
            backpack::BackpackItem,
            layout::AddCanvasItem,
            nodes::{CanvasNodeChild, shapes::CanvasRect},
        },
        desktops::Desktops,
        horizontal_dnd::{AddChild, ChildCount},
        vertical_dnd::VerticalDnD,
    },
    primitives::{
        checkout,
        file_browser::FileBrowser,
        interaction::TakeClicked,
        number::{Float, Integer},
        shapes::{Circle, Path},
        text::{CodeEditor, GetCommittedText, Label, LabelEditable, SetText},
        typst::TypstEditor,
    },
};

/// The sidebar's tabs, in order. `tab` is an index into this.
const TABS: [&str; 4] = ["Prototypes", "Prelude", "History", "Settings"];
const TAB_PROTOTYPES: usize = 0;
const TAB_PRELUDE: usize = 1;
const TAB_HISTORY: usize = 2;
const TAB_SETTINGS: usize = 3;

/// The gap between tabs.
const TAB_GAP: f32 = 4.0;

/// How a tab looks: filled and outlined when it is the one showing, plain when it is not.
fn tab_style(open: bool) -> SetButtonStyle {
    if open {
        SetButtonStyle {
            fill_color: Color::rgba(70, 130, 180, 30),
            border: Stroke::new(1.0, Color::rgb(70, 130, 180)),
            text_color: Color::rgb(40, 80, 120),
        }
    } else {
        SetButtonStyle {
            fill_color: Color::TRANSPARENT,
            border: Stroke::NONE,
            text_color: Color::gray(110),
        }
    }
}

#[utils::dynamic_type]
#[utils::portable]
pub struct CanvasSidebar {
    /// A back-reference to the root.
    #[uid_ref]
    desktops: NodeUid<Desktops>,
    /// Which tab is showing, as an index.
    tab: usize,
    /// One button per tab, styled to show which one is open.
    tab_buttons: Vec<NodeUid<Button>>,

    buttons: Vec<NodeUid<Button>>,
    backpack: NodeUid<VerticalDnD>,

    python_prelude: NodeUid<CodeEditor>,
    prelude_ide_button: NodeUid<Button>,
    /// Where the prelude is checked out for external editing.
    #[dynamic(skip)]
    prelude_checkout: Transient<checkout::Checkout>,
}

#[utils::dynamic_methods]
impl CanvasSidebar {
    /// Labels for the option buttons, in order. The button at index `i` inserts
    /// the node produced by [`CanvasSidebar::dispatch`] for that index.
    pub const OPTIONS: [&'static str; 11] = [
        "Text",
        "Integer",
        "Float",
        "Rect",
        "Circle",
        "Typst",
        "Lambda",
        "Canvas Lambda",
        "File",
        "Polygon",
        "Line",
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
        let tab_buttons = TABS
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let style = tab_style(i == TAB_PROTOTYPES);
                Button::build_with(ws.clone(), Label::new((*name).to_owned()), |b| {
                    b.label.font = Font::proportional(13.0);
                    b.label.color = style.text_color;
                    b.padding = 5.0;
                    b.corner_radius = 4.0;
                    b.fill_color = style.fill_color;
                    b.border = style.border;
                })
            })
            .collect();
        let backpack = VerticalDnD::build(ws.clone(), Vec::new(), 4.0);
        let mut prelude = CodeEditor::new(String::new(), "python".to_owned());
        prelude.fill = true;
        prelude.font_size = 12.0;
        let python_prelude = ws.insert_node(prelude);
        let prelude_ide_button =
            Button::build_with(ws.clone(), Label::new("Open in IDE".to_owned()), |b| {
                b.corner_radius = 5.0;
                b.fill_width = true;
            });
        ws.insert_node(Self {
            desktops,
            tab: TAB_PROTOTYPES,
            tab_buttons,
            buttons,
            backpack,
            python_prelude,
            prelude_ide_button,
            prelude_checkout: Transient::default(),
        })
    }

    /// The insert action for the option at `index`.
    fn dispatch(&self, index: usize, ws: WorkspaceActionHandle) -> Option<Action> {
        let (child, size) = Self::prototype(index, ws)?;
        Some(Action {
            dest: self.desktops.erase(),
            description: "Insert new node".into(),
            body: Box::new(AddCanvasItem { child, size }),
        })
    }

    /// The node the option at `index` inserts, and the size it takes.
    /// Paired with [`CanvasSidebar::OPTIONS`] by position.
    #[dynamic(skip)] // builds a node value, not something a script needs
    pub fn prototype(index: usize, ws: WorkspaceActionHandle) -> Option<(Arc<dyn Node>, Vector)> {
        const DEFAULT: Vector = Vector { x: 160.0, y: 40.0 };
        let (child, size): (Arc<dyn Node>, Vector) = match index {
            0 => (
                Arc::new(LabelEditable::new("Text here".to_owned())),
                DEFAULT,
            ),
            1 => (Arc::new(Integer::new(0)), Vector { x: 80.0, y: 32.0 }),
            2 => (Arc::new(Float::new(0.0)), Vector { x: 80.0, y: 32.0 }),
            3 => (Arc::new(CanvasRect), DEFAULT),
            4 => (
                Arc::new(Circle::new(40.0, Color::rgb(120, 170, 220))),
                Vector { x: 80.0, y: 80.0 },
            ),
            5 => (
                Arc::new(TypstEditor::new(ws)),
                Vector { x: 280.0, y: 220.0 },
            ),
            6 => (Arc::new(Lambda::new(ws)), Vector { x: 420.0, y: 340.0 }),
            7 => (
                Arc::new(CanvasLambda::new(ws)),
                Vector { x: 280.0, y: 220.0 },
            ),
            8 => (
                Arc::new(FileBrowser::new(ws)),
                Vector { x: 320.0, y: 240.0 },
            ),
            9 => (
                Arc::new(Path::polygon(
                    vec![
                        Vector::new(0.0, 0.0),
                        Vector::new(90.0, 0.0),
                        Vector::new(90.0, 90.0),
                        Vector::new(0.0, 90.0),
                    ],
                    Path::default_fill(),
                    Stroke::new(2.0, Color::rgb(60, 90, 130)),
                )),
                Vector { x: 90.0, y: 90.0 },
            ),
            10 => (
                Arc::new(Path::polyline(
                    vec![Vector::new(0.0, 0.0), Vector::new(140.0, 60.0)],
                    Stroke::new(2.5, Color::rgb(80, 80, 90)),
                )),
                Vector { x: 140.0, y: 60.0 },
            ),
            _ => return None,
        };
        Some((child, size))
    }

    /// Check the prelude out to a file and open it in the user's editor.
    #[dynamic(skip)] // takes a borrowed context
    fn edit_prelude_externally(&self, ctx: NodeContext) {
        let source = ctx
            .workspace
            .send_request(self.python_prelude, GetCommittedText {})
            .unwrap_or_default();
        // The prelude runs before anything is wired, so it is handed no globals.
        match checkout::open(&ctx.id.key(), &source, &[]) {
            Ok(open) => self.prelude_checkout.set(open),
            Err(e) => eprintln!("could not check the prelude out: {e}"),
        }
    }

    /**
        Pull in edits made to the checked-out prelude.
        The file wins while it is checked out.
    */
    fn poll_prelude_checkout(&self, ctx: NodeContext) {
        let Some(current) = self.prelude_checkout.val().clone() else {
            return;
        };
        let Some(pulled) = checkout::poll(&current) else {
            return;
        };
        self.prelude_checkout.set(pulled.checkout);
        ctx.workspace.submit_action(
            self.python_prelude,
            "Pulled external prelude edits",
            SetText {
                value: pulled.source,
            },
        );
    }
}

/// A section heading inside a tab.
fn heading(text: &str) -> Label {
    let mut label = Label::new(text.to_owned());
    label.font = Font::proportional(11.0);
    label.color = Color::gray(120);
    label
}

/// Muted body text, for a hint or an empty tab.
fn muted(text: &str) -> Label {
    let mut label = Label::new(text.to_owned());
    label.font = Font::proportional(12.0);
    label.color = Color::gray(150);
    label.singleline = false;
    label
}

impl CanvasSidebar {
    /// Draw the tab strip at `origin`, returning the height it took.
    fn draw_tabs(&self, ctx: &mut DrawContext, origin: ScreenPos, width: f32) -> f32 {
        // Wrapping, so a narrow sidebar stacks the tabs rather than clipping them.
        let strip = HorizontalLayout {
            children: self
                .tab_buttons
                .iter()
                .map(|b| LayoutChild::from(*b))
                .collect(),
            spacing: TAB_GAP,
            allow_wrap: true,
        };
        let drawn = ctx.draw_node(
            &strip,
            DrawConstraints {
                pos: origin,
                x: Some(AxisConstraint::AtMost(width)),
                y: None,
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );

        for (i, &tab) in self.tab_buttons.iter().enumerate() {
            if ctx
                .node
                .workspace
                .send_request(tab.erase(), TakeClicked)
                .unwrap_or(false)
                && self.tab != i
            {
                ctx.submit_action_for_self::<Self, _>(OpenSidebarTab { tab: i }, "Opened a tab");
            }
        }

        drawn.region().map(|r| r.size().y).unwrap_or(0.0)
    }

    /// Draw the Primitives and Backpack sections into the content area.
    fn draw_prototypes(&self, ctx: &mut DrawContext, origin: ScreenPos, size: Vector) -> f32 {
        const GAP: f32 = 10.0;
        let mut y = 0.0;

        let section = |ctx: &mut DrawContext, title: &str, y: &mut f32| {
            let drawn = ctx.draw_node(
                &heading(title),
                DrawConstraints {
                    pos: origin + Vector { x: 0.0, y: *y },
                    x: Some(AxisConstraint::AtMost(size.x)),
                    y: None,
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
            );
            *y += drawn.region().map(|r| r.size().y).unwrap_or(14.0) + 4.0;
        };

        section(ctx, "Primitives", &mut y);
        let options = HorizontalLayout {
            children: self.buttons.iter().map(|b| LayoutChild::from(*b)).collect(),
            spacing: GAP,
            allow_wrap: true,
        };
        let drawn = ctx.draw_node(
            &options,
            DrawConstraints {
                pos: origin + Vector { x: 0.0, y },
                x: Some(AxisConstraint::AtMost(size.x)),
                y: Some(AxisConstraint::AtMost((size.y - y).max(0.0))),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );
        y += drawn.region().map(|r| r.size().y).unwrap_or(0.0) + GAP + 4.0;

        for (i, &btn) in self.buttons.iter().enumerate() {
            if ctx
                .node
                .workspace
                .send_request(btn.erase(), TakeClicked)
                .unwrap_or(false)
                && let Some(action) = self.dispatch(i, ctx.node.workspace.action_handle())
            {
                ctx.node.workspace.submit_action_dyn(action);
            }
        }

        section(ctx, "Backpack", &mut y);
        let remaining = (size.y - y).max(0.0);
        let kept = ctx
            .node
            .workspace
            .send_request(self.backpack, ChildCount)
            .unwrap_or(0);
        if kept == 0 {
            let drawn = ctx.draw_node(
                &muted("Nothing kept yet. Add a node from its inspector."),
                DrawConstraints {
                    pos: origin + Vector { x: 0.0, y },
                    x: Some(AxisConstraint::AtMost(size.x)),
                    y: Some(AxisConstraint::AtMost(remaining)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
            );
            y += drawn.region().map(|r| r.size().y).unwrap_or(0.0);
        } else {
            // Scrolled: a backpack grows, and the sidebar does not.
            ctx.draw_node(
                &ScrollLayout::vertical(LayoutChild::Id(self.backpack.erase()))
                    .with_id_salt("dex_backpack"),
                DrawConstraints {
                    pos: origin + Vector { x: 0.0, y },
                    x: Some(AxisConstraint::Exactly(size.x)),
                    y: Some(AxisConstraint::Exactly(remaining)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
            );
            y += remaining;
        }
        y
    }

    /// Draw the prelude editor and its external-editor button.
    fn draw_prelude(&self, ctx: &mut DrawContext, origin: ScreenPos, size: Vector) -> f32 {
        const GAP: f32 = 6.0;
        let mut y = 0.0;

        let drawn = ctx.draw_node(
            &heading("Python prelude"),
            DrawConstraints {
                pos: origin,
                x: Some(AxisConstraint::AtMost(size.x)),
                y: None,
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );
        y += drawn.region().map(|r| r.size().y).unwrap_or(14.0) + GAP;

        let drawn = ctx.draw_workspace_node(
            self.prelude_ide_button.erase(),
            DrawConstraints {
                pos: origin + Vector { x: 0.0, y },
                x: Some(AxisConstraint::Exactly(size.x)),
                y: None,
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );
        y += drawn
            .and_then(|r| r.region())
            .map(|r| r.size().y)
            .unwrap_or(24.0)
            + GAP;

        if ctx
            .node
            .workspace
            .send_request(self.prelude_ide_button.erase(), TakeClicked)
            .unwrap_or(false)
        {
            self.edit_prelude_externally(ctx.node);
        }

        ctx.draw_workspace_node(
            self.python_prelude.erase(),
            DrawConstraints {
                pos: origin + Vector { x: 0.0, y },
                x: Some(AxisConstraint::Exactly(size.x)),
                y: Some(AxisConstraint::Exactly((size.y - y).max(0.0))),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );
        size.y
    }

    /// Draw a tab that has nothing in it yet.
    fn draw_placeholder(
        &self,
        ctx: &mut DrawContext,
        origin: ScreenPos,
        size: Vector,
        text: &str,
    ) -> f32 {
        ctx.draw_node(
            &muted(text),
            DrawConstraints {
                pos: origin,
                x: Some(AxisConstraint::AtMost(size.x)),
                y: Some(AxisConstraint::AtMost(size.y)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        )
        .region()
        .map(|r| r.size().y)
        .unwrap_or(0.0)
    }
}

#[utils::dynamic_node]
impl Node for CanvasSidebar {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Canvas Sidebar".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const PADDING: f32 = 5.0;

        let origin = ctx.constraints.pos;
        let avail_w = ctx
            .constraints
            .x
            .map(|a| a.provided_value())
            .filter(|w| w.is_finite())
            .unwrap_or(200.0);
        let avail_h = ctx
            .constraints
            .y
            .map(|a| a.provided_value())
            .filter(|h| h.is_finite())
            .unwrap_or(0.0);
        let content_w = (avail_w - 2.0 * PADDING).max(0.0);

        let mut y = PADDING;
        y += self.draw_tabs(&mut ctx, origin + Vector { x: PADDING, y }, content_w) + 6.0;

        // The rule under the strip, so the tabs read as chrome and not content.
        Path::span(
            Vector {
                x: content_w,
                y: 0.0,
            },
            Stroke::new(1.0, Color::gray(225)),
        )
        .paint(ctx.ui.painter(), origin + Vector { x: PADDING, y });
        y += 8.0;

        let content_origin = origin + Vector { x: PADDING, y };
        let content_size = Vector {
            x: content_w,
            y: (avail_h - y - PADDING).max(0.0),
        };
        let used = match self.tab {
            TAB_PROTOTYPES => self.draw_prototypes(&mut ctx, content_origin, content_size),
            TAB_PRELUDE => self.draw_prelude(&mut ctx, content_origin, content_size),
            TAB_HISTORY => self.draw_placeholder(
                &mut ctx,
                content_origin,
                content_size,
                "History will live here.",
            ),
            TAB_SETTINGS => self.draw_placeholder(
                &mut ctx,
                content_origin,
                content_size,
                "Settings will live here.",
            ),
            _ => 0.0,
        };

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(
                origin,
                Vector {
                    x: avail_w,
                    y: (y + used + PADDING).max(avail_h),
                },
            )),
        }
    }

    fn tick(&self, ctx: NodeContext) {
        // Polled off the draw, so an external edit lands whichever tab is open.
        self.poll_prelude_checkout(ctx);
    }

    fn on_delete(&self, ctx: NodeContext) {
        for btn in &self.buttons {
            ctx.workspace.delete_node(btn.erase());
        }
        for tab in &self.tab_buttons {
            ctx.workspace.delete_node(tab.erase());
        }
        ctx.workspace.delete_node(self.backpack.erase());
        ctx.workspace.delete_node(self.python_prelude.erase());
        ctx.workspace.delete_node(self.prelude_ide_button.erase());
    }
}

/// Strip the article a type name leads with, for use as a name.
fn short_name(type_name: &str) -> String {
    type_name
        .strip_prefix("An ")
        .or_else(|| type_name.strip_prefix("A "))
        .unwrap_or(type_name)
        .to_owned()
}

defhandlers! {
    CanvasSidebar {
        actions: [
            // Show one of the sidebar's tabs, and mark it in the strip.
            OpenSidebarTab { tab: usize } => (this, a, ctx) {
                if a.tab < TABS.len() {
                    this.tab = a.tab;
                    for (i, &button) in this.tab_buttons.iter().enumerate() {
                        ctx.workspace.submit_action(
                            button,
                            "Marked the open tab",
                            tab_style(i == a.tab),
                        );
                    }
                }
            },
            /*
                Keep `node` in the backpack.

                `mirror` decides what a placement makes: a copy of what the node
                is now, or a mirror that keeps following it.
            */
            StoreInBackpack { node: NodeUid, size: Vector, mirror: bool } => (this, a, ctx) {
                let ws = ctx.workspace;
                // A canvas item stands for what it wraps: the frame is the canvas's, not the backpack's.
                let content = ws.send_request(a.node, CanvasNodeChild).unwrap_or(a.node);
                let content_ctx = NodeContext { id: content, workspace: ws };
                let name = ws
                    .get_node(content)
                    .map(|node| short_name(&node.type_name(content_ctx)))
                    .unwrap_or_else(|| "Item".to_owned());

                let handle = ws.action_handle();
                let sidebar = ctx.id.cast();
                let item = if a.mirror {
                    BackpackItem::mirror(handle, sidebar, content, name, a.size)
                } else {
                    // Detached from the canvas, so editing the original leaves this one alone.
                    let template = ws.deep_clone(content);
                    BackpackItem::template(handle, sidebar, template, name, a.size)
                };
                ws.submit_action(
                    this.backpack,
                    "Added to the backpack",
                    AddChild { child: item.erase() },
                );

                // Show what just happened.
                ctx.workspace.submit_action(
                    ctx.id.cast::<Self>(),
                    "Opened the backpack",
                    OpenSidebarTab { tab: TAB_PROTOTYPES },
                );
            },
        ],
        requests: [
            SidebarPythonPrelude => (this, _q, ctx): String {
                ctx.workspace.send_request(this.python_prelude, GetCommittedText {}).unwrap_or_default()
            },
            // The list the backpack's entries live in.
            BackpackList => (this, _q): NodeUid<VerticalDnD> { this.backpack },
        ]
    }
}
