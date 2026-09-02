use dex_core::prelude::*;
use utils::Transient;

use crate::{
    composites::{
        button::{Button, SetButtonLabel, SetButtonStyle},
        lambda::{CanvasLambda, Lambda},
    },
    layouts::{
        Bordered, HorizontalLayout, LayoutChild, ScrollLayout,
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
        file_browser::{BrowseFor, FileBrowser, TakePickedPath},
        interaction::TakeClicked,
        number::{Float, Integer},
        shapes::{Circle, Path},
        text::{CodeEditor, GetCommittedText, GetText, Label, LabelEditable, SetText},
        typst::TypstEditor,
    },
};

/// The sidebar's tabs, in order. `tab` is an index into this.
const TABS: [&str; 4] = ["Prototypes", "Prelude", "History", "Controls"];
const TAB_PROTOTYPES: usize = 0;
const TAB_PRELUDE: usize = 1;
const TAB_HISTORY: usize = 2;
const TAB_CONTROLS: usize = 3;

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

    /// The global virtual environment.
    venv: String,
    venv_button: NodeUid<Button>,
    venv_clear_button: NodeUid<Button>,
    /// The browser opened to choose one, while it is open.
    venv_browser: Option<NodeUid<FileBrowser>>,
    /// Why the last chosen folder was refused, if it was.
    venv_error: Option<String>,

    /// The command an external editor is launched with.
    editor_field: NodeUid<LabelEditable>,
    /// Last-seen version of `editor_field`, to catch a committed edit in `tick`.
    seen_editor_version: Transient<u64>,

    /// The folder a workspace is saved into and loaded from.
    save_dir: String,
    /// The file name within it.
    save_name: NodeUid<LabelEditable>,
    save_dir_button: NodeUid<Button>,
    save_button: NodeUid<Button>,
    load_button: NodeUid<Button>,
    /// The browser opened to choose the folder, while it is open.
    save_browser: Option<NodeUid<FileBrowser>>,
    /// What the last save or load said, good or bad.
    save_note: Option<String>,
    save_failed: bool,
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
        let venv_button =
            Button::build_with(ws.clone(), Label::new(venv_button_label(false)), |b| {
                b.corner_radius = 4.0;
                b.padding = 4.0;
            });
        let venv_clear_button =
            Button::build_with(ws.clone(), Label::new("Clear".to_owned()), |b| {
                b.corner_radius = 4.0;
                b.padding = 4.0;
            });
        // Starts on the default, so the field always shows what will run rather
        // than an empty box that means "whatever the default happens to be".
        let mut save_name_field = LabelEditable::new("workspace.dex".to_owned());
        save_name_field.font = Font::monospaced(12.0);
        save_name_field.shrink_to_text = false;
        let save_name = ws.insert_node(save_name_field);
        let small = |label: &str| {
            Button::build_with(ws.clone(), Label::new(label.to_owned()), |b| {
                b.corner_radius = 4.0;
                b.padding = 4.0;
            })
        };
        let save_dir_button = small(&save_dir_button_label(false));
        let save_button = small("Save");
        let load_button = small("Load");

        let mut editor = LabelEditable::new(crate::settings::DEFAULT_EDITOR.to_owned());
        editor.font = Font::monospaced(12.0);

        editor.shrink_to_text = false;
        let editor_field = ws.insert_node(editor);
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
            venv: String::new(),
            venv_button,
            venv_clear_button,
            venv_browser: None,
            venv_error: None,
            editor_field,
            seen_editor_version: Transient::default(),
            save_dir: String::new(),
            save_name,
            save_dir_button,
            save_button,
            load_button,
            save_browser: None,
            save_note: None,
            save_failed: false,
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
/// What the environment button says, given whether the browser is showing.
fn venv_button_label(browsing: bool) -> String {
    if browsing { "Cancel" } else { "Choose…" }.to_owned()
}

/// The same, for the save folder's button.
fn save_dir_button_label(browsing: bool) -> String {
    if browsing { "Cancel" } else { "Folder…" }.to_owned()
}

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

    /// Draw the settings tab.
    fn draw_controls(&self, ctx: &mut DrawContext, origin: ScreenPos, size: Vector) -> f32 {
        const GAP: f32 = 6.0;
        const ROW_GAP: f32 = 4.0;
        let mut y = 0.0;

        let row = |ctx: &mut DrawContext, y: &mut f32, node: &dyn Node, fill: bool| {
            let drawn = ctx.draw_node(
                node,
                DrawConstraints {
                    pos: origin + Vector { x: 0.0, y: *y },
                    x: Some(if fill {
                        AxisConstraint::Exactly(size.x)
                    } else {
                        AxisConstraint::AtMost(size.x)
                    }),
                    y: None,
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
            );
            *y += drawn.region().map(|r| r.size().y).unwrap_or(14.0) + ROW_GAP;
        };

        row(ctx, &mut y, &heading("Global environment"), false);
        let shown = if self.venv.is_empty() {
            "None — scripts import from the interpreter dex was built against.".to_owned()
        } else {
            self.venv.clone()
        };
        row(ctx, &mut y, &muted(&shown), false);
        if let Some(error) = &self.venv_error {
            let mut label = muted(error);
            label.color = Color::rgb(180, 70, 60);
            row(ctx, &mut y, &label, false);
        }

        // Choose, and — only when there is one to clear — Clear.
        let controls = HorizontalLayout {
            children: [
                Some(LayoutChild::from(self.venv_button)),
                (!self.venv.is_empty()).then(|| LayoutChild::from(self.venv_clear_button)),
            ]
            .into_iter()
            .flatten()
            .collect(),
            spacing: ROW_GAP,
            allow_wrap: false,
        };
        row(ctx, &mut y, &controls, false);
        y += GAP;

        // The browser takes the rest of the tab while it is open, except the bottom button.
        if let Some(browser) = self.venv_browser {
            ctx.draw_workspace_node(
                browser.erase(),
                DrawConstraints {
                    pos: origin + Vector { x: 0.0, y },
                    x: Some(AxisConstraint::Exactly(size.x)),
                    y: Some(AxisConstraint::Exactly((size.y - y).max(0.0))),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
            );
            return size.y;
        }

        row(ctx, &mut y, &heading("External editor"), false);
        row(
            ctx,
            &mut y,
            &muted("$1 is the folder and $2 the file. Either one left out is appended."),
            false,
        );
        // Bordered, so it reads as somewhere to type.
        let boxed = |child: LayoutChild| Bordered {
            child,
            padding: 4.0,
            corner_radius: 4.0,
            fill_color: Color::WHITE,
            border_width: 1.0,
            border_color: Color::gray(190),
        };
        row(
            ctx,
            &mut y,
            &boxed(LayoutChild::from(self.editor_field)),
            true,
        );
        y += GAP;

        // -- the workspace file -------------------------------------------
        row(ctx, &mut y, &heading("Workspace"), false);
        let shown = if self.save_dir.is_empty() {
            "No folder chosen.".to_owned()
        } else {
            self.save_dir.clone()
        };
        row(ctx, &mut y, &muted(&shown), false);
        row(
            ctx,
            &mut y,
            &HorizontalLayout {
                children: vec![
                    LayoutChild::from(self.save_dir_button),
                    LayoutChild::from(self.save_button),
                    LayoutChild::from(self.load_button),
                ],
                spacing: ROW_GAP,
                allow_wrap: false,
            },
            false,
        );
        row(ctx, &mut y, &boxed(LayoutChild::from(self.save_name)), true);
        if let Some(note) = &self.save_note {
            let mut label = muted(note);
            if self.save_failed {
                label.color = Color::rgb(180, 70, 60);
            }
            row(ctx, &mut y, &label, false);
        }

        // The folder browser takes what is left, as the environment's does.
        if let Some(browser) = self.save_browser {
            y += GAP;
            ctx.draw_workspace_node(
                browser.erase(),
                DrawConstraints {
                    pos: origin + Vector { x: 0.0, y },
                    x: Some(AxisConstraint::Exactly(size.x)),
                    y: Some(AxisConstraint::Exactly((size.y - y).max(0.0))),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
            );
            return size.y;
        }

        y
    }

    /// Poll the settings controls. Off the draw, so a click lands whichever
    /// tab was showing when it happened.
    fn poll_settings(&self, ctx: NodeContext) {
        let ws = ctx.workspace;
        let taken = |button: NodeUid<Button>| {
            ws.send_request(button.erase(), TakeClicked)
                .unwrap_or(false)
        };

        if taken(self.venv_button) {
            let start = (!self.venv.is_empty()).then(|| self.venv.clone());
            ws.submit_action(
                ctx.id.cast::<Self>(),
                "Choose an environment",
                ToggleVenvBrowser { start },
            );
        }
        if taken(self.venv_clear_button) {
            ws.submit_action(
                ctx.id.cast::<Self>(),
                "Cleared the environment",
                SetVenv {
                    path: String::new(),
                },
            );
        }

        // An open browser is asked whether it has an answer yet.
        if let Some(browser) = self.venv_browser
            && let Some(path) = ws.send_request(browser, TakePickedPath).flatten()
        {
            ws.submit_action(
                ctx.id.cast::<Self>(),
                "Chose an environment",
                SetVenv { path },
            );
        }
        if let Some(browser) = self.save_browser
            && let Some(path) = ws.send_request(browser, TakePickedPath).flatten()
        {
            ws.submit_action(ctx.id.cast::<Self>(), "Chose a folder", SetSaveDir { path });
        }

        if taken(self.save_dir_button) {
            let start = (!self.save_dir.is_empty()).then(|| self.save_dir.clone());
            ws.submit_action(
                ctx.id.cast::<Self>(),
                "Choose a folder",
                ToggleSaveBrowser { start },
            );
        }
        if taken(self.save_button) {
            ws.submit_action(ctx.id.cast::<Self>(), "Save the workspace", SaveWorkspace);
        }
        if taken(self.load_button) {
            ws.submit_action(ctx.id.cast::<Self>(), "Load a workspace", RequestLoad);
        }

        // The editor command, when the field commits an edit.
        let version = ws.version_of(self.editor_field.erase());
        let seen = *self.seen_editor_version.val();
        self.seen_editor_version.set(version);
        if let Some(previous) = seen
            && previous != version
        {
            let typed = ws
                .send_request(self.editor_field, GetText)
                .unwrap_or_default();
            crate::settings::set_editor_command(typed);
        }
    }

    /// Where a save goes: the chosen folder and the typed name, or why not.
    fn save_path(&self, ws: &Workspace) -> Result<std::path::PathBuf, String> {
        if self.save_dir.is_empty() {
            return Err("Choose a folder first.".to_owned());
        }
        let name = ws.send_request(self.save_name, GetText).unwrap_or_default();
        let name = name.trim();
        if name.is_empty() {
            return Err("Give the file a name.".to_owned());
        }
        Ok(std::path::Path::new(&self.save_dir).join(name))
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
            TAB_CONTROLS => self.draw_controls(&mut ctx, content_origin, content_size),
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
        self.poll_settings(ctx);
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
        ctx.workspace.delete_node(self.venv_button.erase());
        ctx.workspace.delete_node(self.venv_clear_button.erase());
        ctx.workspace.delete_node(self.editor_field.erase());
        ctx.workspace.delete_node(self.save_name.erase());
        for button in [self.save_dir_button, self.save_button, self.load_button] {
            ctx.workspace.delete_node(button.erase());
        }
        for browser in [self.venv_browser, self.save_browser].into_iter().flatten() {
            ctx.workspace.delete_node(browser.erase());
        }
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
            // Show or hide the browser used to choose an environment.
            ToggleVenvBrowser { start: Option<String> } => (this, a, ctx) {
                let ws = ctx.workspace.action_handle();
                match this.venv_browser.take() {
                    Some(open) => ctx.workspace.delete_node(open.erase()),
                    None => {
                        let dir = a.start.clone().unwrap_or_else(FileBrowser::default_dir_string);
                        this.venv_browser = Some(ws.insert_node(FileBrowser::picker(
                            ws.clone(),
                            dir,
                            BrowseFor::PickedDirectory,
                        )));
                    }
                }
                this.venv_error = None;
                ctx.workspace.submit_action(
                    this.venv_button,
                    "Retitled the environment button",
                    SetButtonLabel { text: venv_button_label(this.venv_browser.is_some()) },
                );
            },
            // Take `path` as the global environment, or say why it cannot be.
            SetVenv { path: String } => (this, a, ctx) {
                let chosen = a.path.trim();
                let wanted = (!chosen.is_empty()).then(|| ::std::path::PathBuf::from(chosen));
                match crate::settings::set_venv(wanted) {
                    Ok(()) => {
                        this.venv = chosen.to_owned();
                        this.venv_error = None;
                        // Answered, so the browser has nothing left to do.
                        if let Some(open) = this.venv_browser.take() {
                            ctx.workspace.delete_node(open.erase());
                            ctx.workspace.submit_action(
                                this.venv_button,
                                "Retitled the environment button",
                                SetButtonLabel { text: venv_button_label(false) },
                            );
                        }
                    }
                    // Left open on a refusal.
                    Err(why) => this.venv_error = Some(why),
                }
            },
            // Show or hide the browser used to choose a save folder.
            ToggleSaveBrowser { start: Option<String> } => (this, a, ctx) {
                let ws = ctx.workspace.action_handle();
                match this.save_browser.take() {
                    Some(open) => ctx.workspace.delete_node(open.erase()),
                    None => {
                        let dir = a.start.clone().unwrap_or_else(FileBrowser::default_dir_string);
                        this.save_browser = Some(ws.insert_node(FileBrowser::picker(
                            ws.clone(),
                            dir,
                            BrowseFor::PickedDirectory,
                        )));
                    }
                }
                ctx.workspace.submit_action(
                    this.save_dir_button,
                    "Retitled the folder button",
                    SetButtonLabel { text: save_dir_button_label(this.save_browser.is_some()) },
                );
            },
            // Take `path` as the folder to save into and load from.
            SetSaveDir { path: String } => (this, a, ctx) {
                this.save_dir = a.path.trim().to_owned();
                this.save_note = None;
                this.save_failed = false;
                if let Some(open) = this.save_browser.take() {
                    ctx.workspace.delete_node(open.erase());
                    ctx.workspace.submit_action(
                        this.save_dir_button,
                        "Retitled the folder button",
                        SetButtonLabel { text: save_dir_button_label(false) },
                    );
                }
            },
            // Write the whole workspace to the chosen file.
            SaveWorkspace => (this, _a, ctx) {
                match this.save_path(ctx.workspace) {
                    Err(why) => {
                        this.save_note = Some(why);
                        this.save_failed = true;
                    }
                    Ok(path) => match ctx.workspace.save_to(&path) {
                        Ok(()) => {
                            this.save_note = Some(format!("Saved to {}", path.display()));
                            this.save_failed = false;
                        }
                        Err(e) => {
                            this.save_note = Some(e.to_string());
                            this.save_failed = true;
                        }
                    },
                }
            },
            // Read the chosen file and, if it is a workspace, swap it in.
            RequestLoad => (this, _a, ctx) {
                let read = this
                    .save_path(ctx.workspace)
                    .and_then(|path| {
                        Workspace::read_from(&path).map_err(|e| e.to_string())
                    });
                match read {
                    Ok((root, registry)) => {
                        this.save_note = None;
                        this.save_failed = false;
                        ctx.workspace.submit_action_dyn(Action {
                            dest: NodeUid::nil(),
                            description: "Loaded a workspace".into(),
                            body: Box::new(LoadWorkspace { root, registry }),
                        });
                    }
                    Err(why) => {
                        this.save_note = Some(why);
                        this.save_failed = true;
                    }
                }
            },
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
