use std::fmt::Debug;
use std::path::{Path, PathBuf};

use dex_core::prelude::*;
use dex_core::theme;
use utils::Transient;

use crate::{
    composites::button::Button,
    layouts::{
        Bordered, HorizontalLayout, LayoutChild, ScrollLayout, VerticalLayout, error::ErrorLayout,
    },
    primitives::{
        icon::Glyph,
        image::Image,
        interaction::WasClicked,
        table::Table,
        text::{GetText, Label, LabelEditable, SetText},
    },
};

/// What clicking through a browser is for.
#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
pub enum BrowseFor {
    /// A file to open in place: the browser becomes the file's content.
    OpenedFile,
    /// A file, reported to whoever asked for it.
    PickedFile,
    /// A directory, reported to whoever asked for it.
    PickedDirectory,
}

#[utils::portable]
pub struct FileBrowser {
    cur_dir: String,

    /// What a pick is for.
    mode: BrowseFor,
    /// What was picked, until it is taken.
    picked: Transient<String>,
    /// How tall the footer button drew last frame.
    footer_height: Transient<f32>,
    /// Whether dotfiles are listed. A `.venv` is the case this exists for —
    /// the environment you want to pick is usually one.
    show_hidden: bool,

    /// "Up one folder" button.
    up_button: NodeUid<Button>,
    /// Editable display of the current path.
    path_field: NodeUid<LabelEditable>,
    /// "Use this folder", when a directory is what is being picked.
    choose_button: Option<NodeUid<Button>>,

    /// One button per directory entry.
    rows: Vec<NodeUid<Button>>,
    entry_paths: Vec<String>,
    entry_is_dir: Vec<bool>,

    /// Last-seen version of `path_field`, to detect a committed edit in `tick`.
    seen_path_version: Transient<u64>,
}

impl FileBrowser {
    /// The directory a fresh browser starts in.
    fn default_dir() -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    pub fn default_dir_string() -> String {
        Self::default_dir().to_string_lossy().into_owned()
    }

    /// A browser starting in the default directory, opening what is picked.
    pub fn new(ws: WorkspaceActionHandle) -> FileBrowser {
        Self::new_at(ws, Self::default_dir_string())
    }

    /// A browser listing `dir`, opening what is picked.
    pub fn new_at(ws: WorkspaceActionHandle, dir: String) -> FileBrowser {
        Self::build(ws, dir, BrowseFor::OpenedFile, false)
    }

    /// A browser that holds what is picked for the asking, rather than becoming it.
    pub fn picker(ws: WorkspaceActionHandle, dir: String, mode: BrowseFor) -> FileBrowser {
        Self::build(ws, dir, mode, true)
    }

    /// A browser listing `dir`, building its header and row buttons into `ws`.
    fn build(
        ws: WorkspaceActionHandle,
        dir: String,
        mode: BrowseFor,
        show_hidden: bool,
    ) -> FileBrowser {
        let up_button = Button::build_icon(ws.clone(), Glyph::ArrowUp);

        let mut path_label = LabelEditable::new(dir.clone());
        // A path is chrome above the list, not content.
        path_label.font = theme::text_small();
        path_label.color = theme::INK_MUTED;
        let path_field = ws.insert_node(path_label);

        let choose_button = matches!(mode, BrowseFor::PickedDirectory).then(|| {
            Button::build_with(ws.clone(), Label::new("Use this folder".to_owned()), |b| {
                b.label.font = theme::text();
                b.fill_width = true;
            })
        });

        let (rows, entry_paths, entry_is_dir) = Self::build_rows(&ws, &dir, show_hidden);

        FileBrowser {
            cur_dir: dir,
            mode,
            picked: Transient::default(),
            footer_height: Transient::default(),
            show_hidden,
            up_button,
            path_field,
            choose_button,
            rows,
            entry_paths,
            entry_is_dir,
            seen_path_version: Transient::default(),
        }
    }

    /// Directory contents; folders first, then files, by name.
    fn read_entries(dir: &Path, show_hidden: bool) -> std::io::Result<Vec<(PathBuf, bool)>> {
        let mut entries: Vec<(PathBuf, bool)> = std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .filter(|entry| show_hidden || !entry.file_name().as_encoded_bytes().starts_with(b"."))
            .map(|e| {
                let path = e.path();
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                (path, is_dir)
            })
            .collect();
        entries.sort_by(|(path_a, is_dir_a), (path_b, is_dir_b)| {
            is_dir_b.cmp(is_dir_a).then_with(|| {
                path_a
                    .file_name()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .cmp(&path_b.file_name().unwrap_or_default().to_ascii_lowercase())
            })
        });
        Ok(entries)
    }

    /// Build one row button per entry of `dir`.
    fn build_rows(
        ws: &WorkspaceActionHandle,
        dir: &str,
        show_hidden: bool,
    ) -> (Vec<NodeUid<Button>>, Vec<String>, Vec<bool>) {
        let entries = Self::read_entries(Path::new(dir), show_hidden).unwrap_or_default();

        let mut rows = Vec::with_capacity(entries.len());
        let mut entry_paths = Vec::with_capacity(entries.len());
        let mut entry_is_dir = Vec::with_capacity(entries.len());

        for (path, is_dir) in entries {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut label = Label::new(name);
            label.font = theme::text();
            // A flat, full-width list row: no border, transparent, snug padding.
            let button = Button::build_with(ws.clone(), label, |b| {
                b.icon = Some(if is_dir { Glyph::Folder } else { Glyph::File });
                b.icon_gap = theme::SPACE_MD;
                b.padding = theme::SPACE_SM;
                b.padding_x = theme::SPACE_XS;
                b.corner_radius = theme::RADIUS_SM;
                b.fill_color = Color::TRANSPARENT;
                b.border = Stroke::NONE;
                b.fill_width = true;
            });
            rows.push(button);
            entry_paths.push(path.to_string_lossy().into_owned());
            entry_is_dir.push(is_dir);
        }

        (rows, entry_paths, entry_is_dir)
    }

    /// Build the node the picked `path` should become, reading and parsing it.
    fn node_for(
        ws: &WorkspaceActionHandle,
        owner: NodeUid,
        dir: &str,
        path: &Path,
    ) -> Arc<dyn Node> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        match ext.as_str() {
            "svg" => match std::fs::read_to_string(path) {
                Ok(src) => Arc::new(Image::from_svg(src)),
                Err(e) => Self::open_error(ws, owner, dir, path, format!("{e:?}")),
            },
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "tiff" | "tif" => {
                match std::fs::read(path) {
                    Ok(bytes) => Arc::new(Image::from_bytes(bytes)),
                    Err(e) => Self::open_error(ws, owner, dir, path, format!("{e:?}")),
                }
            }
            "csv" | "tsv" | "tab" | "psv" => match Table::from_file(path) {
                Ok(table) => Arc::new(table),
                Err(e) => Self::open_error(ws, owner, dir, path, format!("{e:?}")),
            },
            // Everything else is treated as text; non-UTF-8 content is reported.
            _ => match std::fs::read(path) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => {
                        let mut label = Label::new(text);
                        label.singleline = false;
                        // Long files overflow the node; make the text scroll.
                        Arc::new(ScrollLayout::vertical(LayoutChild::Node(Arc::new(label))))
                    }
                    Err(_) => {
                        Self::open_error(ws, owner, dir, path, "not a UTF-8 text file".to_owned())
                    }
                },
                Err(e) => Self::open_error(ws, owner, dir, path, format!("{e:?}")),
            },
        }
    }

    fn open_error(
        ws: &WorkspaceActionHandle,
        owner: NodeUid,
        dir: &str,
        path: &Path,
        reason: impl Debug,
    ) -> Arc<dyn Node> {
        let message = format!("Could not open {}: {reason:?}", path.display());
        Arc::new(FileOpenError::build(ws, owner, dir.to_owned(), message))
    }
}

#[utils::dynamic_node(skip)]
impl Node for FileBrowser {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A File Browser".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        const HEADER_GAP: f32 = theme::SPACE_MD;
        const ROW_GAP: f32 = 1.0;
        /// The band kept for the "Use this folder" button.
        const FOOTER_FALLBACK: f32 = 30.0;
        /// Breathing room under the button, so it is not flush with the edge.
        const FOOTER_PAD: f32 = theme::SPACE_SM;

        // Header: up button and current path.
        let header = HorizontalLayout {
            children: vec![
                LayoutChild::from(self.up_button),
                LayoutChild::from(self.path_field),
            ],
            spacing: HEADER_GAP,
            allow_wrap: false,
        };

        // A scrollable column of the entry buttons.
        let list_column = VerticalLayout {
            children: self.rows.iter().map(|r| LayoutChild::from(*r)).collect(),
            spacing: ROW_GAP,
            fill_last: false,
        };
        let list = ScrollLayout::vertical(LayoutChild::Node(Arc::new(list_column)))
            .with_id_salt(ctx.node.id);

        let body = VerticalLayout {
            children: vec![
                LayoutChild::Node(Arc::new(header)),
                LayoutChild::Node(Arc::new(list)),
            ],
            spacing: HEADER_GAP,
            // The list claims the remaining height.
            fill_last: true,
        };
        let bordered = Bordered {
            child: LayoutChild::Node(Arc::new(body)),
            padding: theme::SPACE_MD,
            corner_radius: theme::RADIUS_LG,
            fill_color: Color::WHITE,
            border_width: 1.0,
            border_color: theme::LINE,
        };
        let constraints = ctx.constraints;
        // The footer's band is taken out of the panel before it is drawn: a
        // long listing would otherwise push the button past the bottom edge,
        // which is exactly when it is wanted.
        let avail_h = constraints
            .y
            .map(|a| a.provided_value())
            .filter(|h| h.is_finite());
        // What the button took last time it drew.
        let footer_h = self
            .choose_button
            .map(|_| self.footer_height.val().unwrap_or(FOOTER_FALLBACK));
        let band = footer_h.map(|h| h + HEADER_GAP + FOOTER_PAD).unwrap_or(0.0);
        let result = ctx.draw_node(
            &bordered,
            DrawConstraints {
                y: avail_h
                    .map(|h| AxisConstraint::Exactly((h - band).max(0.0)))
                    .or(constraints.y),
                ..constraints
            },
        );

        if let Some(choose) = self.choose_button {
            // Under the list, across the full width.
            let panel_h = result.region().map(|r| r.size().y).unwrap_or(0.0);
            let top = avail_h
                .map(|h| h - FOOTER_PAD - footer_h.unwrap_or(FOOTER_FALLBACK))
                .unwrap_or(panel_h + HEADER_GAP);
            let drawn = ctx.draw_workspace_node(
                choose.erase(),
                DrawConstraints {
                    pos: constraints.pos + Vector { x: 0.0, y: top },
                    x: constraints.x,
                    y: None,
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: false,
                },
            );
            if let Some(height) = drawn.and_then(|r| r.region()).map(|r| r.size().y) {
                self.footer_height.set(height);
            }
        }

        // Poll the (now-drawn) buttons and dispatch navigation / open.
        if ctx
            .node
            .workspace
            .send_request(self.up_button.erase(), WasClicked)
            .unwrap_or(false)
            && let Some(parent) = Path::new(&self.cur_dir).parent()
        {
            ctx.submit_action_for_self::<Self, _>(
                Navigate {
                    dir: parent.to_string_lossy().into_owned(),
                },
                "Open parent folder",
            );
        }

        if let Some(choose) = self.choose_button
            && ctx
                .node
                .workspace
                .send_request(choose.erase(), WasClicked)
                .unwrap_or(false)
        {
            self.picked.set(self.cur_dir.clone());
        }

        for i in 0..self.rows.len() {
            let clicked = ctx
                .node
                .workspace
                .send_request(self.rows[i].erase(), WasClicked)
                .unwrap_or(false);
            if !clicked {
                continue;
            }
            let path = self.entry_paths[i].clone();
            // A folder is always somewhere to go.
            if self.entry_is_dir[i] {
                ctx.submit_action_for_self::<Self, _>(Navigate { dir: path }, "Open folder");
                continue;
            }
            match self.mode {
                BrowseFor::OpenedFile => {
                    ctx.submit_action_for_self::<Self, _>(OpenPath { path }, "Open file")
                }
                BrowseFor::PickedFile => self.picked.set(path),
                // Files are not what was asked for.
                BrowseFor::PickedDirectory => {}
            }
        }

        match (avail_h, self.choose_button) {
            // Reported with the footer, or whatever laid the browser out would
            // put the next thing over the button.
            (Some(h), Some(_)) => DrawResult::Complete {
                region: Some(ScreenRegion::from_min_size(
                    constraints.pos,
                    Vector {
                        x: result.region().map(|r| r.size().x).unwrap_or(0.0),
                        y: h,
                    },
                )),
            },
            _ => result,
        }
    }

    fn tick(&self, ctx: NodeContext) {
        // Navigate when the path field commits a new value.
        let version = ctx.workspace.version_of(self.path_field.erase());
        let seen = *self.seen_path_version.val();
        self.seen_path_version.set(version);
        let Some(prev) = seen else {
            // First observation: record the baseline without navigating.
            return;
        };
        if prev == version {
            return;
        }

        let typed = ctx
            .workspace
            .send_request(self.path_field, GetText)
            .unwrap_or_default();
        if typed != self.cur_dir && Path::new(&typed).is_dir() {
            ctx.workspace.submit_action(
                ctx.id.cast::<FileBrowser>(),
                "Navigate to typed path",
                Navigate { dir: typed },
            );
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.up_button.erase());
        ctx.workspace.delete_node(self.path_field.erase());
        if let Some(choose) = self.choose_button {
            ctx.workspace.delete_node(choose.erase());
        }
        for row in &self.rows {
            ctx.workspace.delete_node(row.erase());
        }
    }
}

dex_core::defrequest!(
    /// The path a picker browser picked, consumed by the asking.
    TakePickedPath: Option<String>
);

defhandlers! { FileBrowser {
    actions: [
        // Re-list `dir`: rebuild the row buttons, and sync the path display.
        Navigate { dir: String } => (this, s, ctx) {
            for row in this.rows.drain(..) {
                ctx.workspace.delete_node(row.erase());
            }
            let (rows, entry_paths, entry_is_dir) = FileBrowser::build_rows(
                &ctx.workspace.action_handle(),
                &s.dir,
                this.show_hidden,
            );
            this.rows = rows;
            this.entry_paths = entry_paths;
            this.entry_is_dir = entry_is_dir;
            this.cur_dir = s.dir.clone();
            ctx.workspace.submit_action(
                this.path_field,
                "Sync path display",
                SetText { value: s.dir },
            );
        },
        // Replace this browser in place with the opened file's content. Children must be cleaned up first.
        OpenPath { path: String } => (this, s, ctx) {
            let owner = ctx.id;
            let ws = ctx.workspace.action_handle();
            let content = FileBrowser::node_for(&ws, owner, &this.cur_dir, Path::new(&s.path));

            ctx.workspace.delete_node(this.up_button.erase());
            ctx.workspace.delete_node(this.path_field.erase());
            for row in &this.rows {
                ctx.workspace.delete_node(row.erase());
            }

            ws.insert_node_at_dyn(owner, content);
        },
    ],
    extern_requests: [
        TakePickedPath => (this, _q): Option<String> { this.picked.val_mut().take() },
    ],
} }

/// The result of failing to open a file.
#[utils::portable]
pub struct FileOpenError {
    /// The workspace slot to restore a browser into on "back".
    #[uid_ref]
    owner: NodeUid,
    /// The directory the browser was in when the file was picked.
    dir: String,
    message: String,
    back_button: NodeUid<Button>,
}

impl FileOpenError {
    fn build(
        ws: &WorkspaceActionHandle,
        owner: NodeUid,
        dir: String,
        message: String,
    ) -> FileOpenError {
        let back_button = Button::build_with(ws.clone(), Label::new("← Back".to_owned()), |b| {
            b.corner_radius = 4.0;
            b.padding = 4.0;
        });
        FileOpenError {
            owner,
            dir,
            message,
            back_button,
        }
    }
}

#[utils::dynamic_node(skip)]
impl Node for FileOpenError {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A File Opening Error".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let controls = HorizontalLayout {
            children: vec![LayoutChild::from(self.back_button)],
            spacing: 0.0,
            allow_wrap: false,
        };
        let body = VerticalLayout {
            children: vec![
                LayoutChild::Node(Arc::new(controls)),
                LayoutChild::Node(Arc::new(ErrorLayout::message(self.message.clone()))),
            ],
            spacing: 6.0,
            fill_last: false,
        };
        let bordered = Bordered {
            child: LayoutChild::Node(Arc::new(body)),
            padding: theme::SPACE_MD,
            corner_radius: theme::RADIUS_LG,
            fill_color: Color::WHITE,
            border_width: 1.0,
            border_color: theme::LINE,
        };
        let constraints = ctx.constraints;
        let result = ctx.draw_node(&bordered, constraints);

        // "Back" restores a browser at `dir` in our slot.
        if ctx
            .node
            .workspace
            .send_request(self.back_button.erase(), WasClicked)
            .unwrap_or(false)
        {
            let ws = ctx.node.workspace.action_handle();
            let browser = FileBrowser::new_at(ws.clone(), self.dir.clone());
            ws.insert_node_at_dyn(self.owner, Arc::new(browser));
            ws.delete_node(self.back_button.erase());
        }

        result
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.back_button.erase());
    }
}

defhandlers! { FileOpenError {} }
