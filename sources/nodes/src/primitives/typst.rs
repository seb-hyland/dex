use dex_core::prelude::*;
use dex_core::theme;
use typst::{
    Library, LibraryExt,
    diag::{FileError, FileResult, SourceDiagnostic},
    ecow::EcoVec,
    foundations::Bytes,
    syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font as TypstFont, FontBook},
    utils::LazyHash,
};
use typst_layout::PagedDocument;
use typst_svg::SvgOptions;
use utils::Transient;

use crate::{
    layouts::error::ErrorLayout,
    primitives::{
        image::Image,
        interaction::{InteractionBox, WasClicked},
        text::{CodeEditor, GetText},
    },
};

#[derive(Clone)]
pub struct IncrementalTypstWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<TypstFont>,
    source: Source,
}

fn fonts() -> Vec<TypstFont> {
    let mut all_fonts = Vec::new();

    let all_font_data: [&'static [u8]; _] = [
        include_bytes!("../../../../assets/Literata/Literata-Regular.ttf"),
        include_bytes!("../../../../assets/Literata/Literata-Bold.ttf"),
        include_bytes!("../../../../assets/Literata/Literata-Italic.ttf"),
        include_bytes!("../../../../assets/Literata/Literata-BoldItalic.ttf"),
        include_bytes!("../../../../assets/Libertinus/LibertinusMath-Regular.ttf"),
    ];

    for font_data in all_font_data {
        let buffer = Bytes::new(font_data);
        for font in TypstFont::iter(buffer) {
            all_fonts.push(font);
        }
    }

    all_fonts
}

impl Default for IncrementalTypstWorld {
    fn default() -> Self {
        let fonts = fonts();

        Self {
            library: LazyHash::new(typst::Library::default()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
            source: Source::new(
                FileId::unique(RootedPath::new(
                    VirtualRoot::Project,
                    VirtualPath::new("typst-source")
                        .expect("Typst vpath should not fail to construct"),
                )),
                String::new(),
            ),
        }
    }
}

impl IncrementalTypstWorld {
    fn update_source(&mut self, source: &str) {
        self.source.replace(source);
    }

    pub fn render(&mut self, code: &str) -> Result<String, EcoVec<SourceDiagnostic>> {
        let header = r#"
    #set page(fill: none, width: auto, height: auto, margin: 10pt)
    #set text(font: "Literata")
    #show math.equation: set text(font: "Libertinus Math")
            "#;
        let full_code = format!("{}\n{}", header.trim(), code);

        self.update_source(&full_code);
        let document: PagedDocument = typst::compile(self).output?;
        let img = typst_svg::svg(&document.pages()[0], &SvgOptions::default());

        Ok(img)
    }
}

impl typst::World for IncrementalTypstWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn main(&self) -> FileId {
        self.source.id()
    }

    fn source(&self, id: FileId) -> Result<Source, FileError> {
        assert_eq!(id, self.source.id());
        Ok(self.source.clone())
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn font(&self, id: usize) -> Option<TypstFont> {
        self.fonts.get(id).cloned()
    }

    fn file(&self, path: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(path.vpath().get_without_slash().into()))
    }

    fn today(
        &self,
        _offset: Option<typst::foundations::Duration>,
    ) -> Option<typst::foundations::Datetime> {
        None
    }
}

#[utils::portable]
#[utils::dynamic_type]
pub struct TypstEditor {
    world: Transient<IncrementalTypstWorld>,

    editor: NodeUid<CodeEditor>,
    click_sensor: NodeUid<InteractionBox>,
    edit_in_progress: Transient<bool>,

    render_cache: Transient<(String, Result<Image, String>)>,
}

#[utils::dynamic_methods]
impl TypstEditor {
    pub fn new(ws: WorkspaceActionHandle) -> Self {
        let mut editor_node = CodeEditor::new("Typst editor".to_owned(), "typst".to_owned());
        editor_node.fill = true;
        let editor = ws.insert_node(editor_node);
        let click_sensor = ws.insert_node(InteractionBox::sensing(false, true, true));

        Self {
            world: Transient::default(),
            editor,
            click_sensor,
            edit_in_progress: Transient::default(),
            render_cache: Transient::default(),
        }
    }

    /// Compile `code`, joining any diagnostics into a single error message.
    fn compile(&self, code: &str) -> Result<Image, String> {
        self.world
            .val_mut_or_else(IncrementalTypstWorld::default)
            .render(code)
            .map(Image::from_svg)
            .map_err(|diags| {
                diags
                    .into_iter()
                    .map(|d| d.message.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
    }

    /// The rendered document as a node (the image, or an error node).
    fn rendered(&self, ctx: &DrawContext) -> Arc<dyn Node> {
        let code = ctx
            .node
            .workspace
            .send_request(self.editor, GetText {})
            .unwrap_or_default();

        let cached = {
            let guard = self.render_cache.val();
            match &*guard {
                Some((prev, result)) if *prev == code => Some(result.clone()),
                _ => None,
            }
        };
        let result = cached.unwrap_or_else(|| {
            let result = self.compile(&code);
            self.render_cache.set((code, result.clone()));
            result
        });

        match result {
            Ok(img) => Arc::new(img) as Arc<dyn Node>,
            Err(msg) => Arc::new(ErrorLayout::message(msg)),
        }
    }
}

#[utils::dynamic_node]
impl Node for TypstEditor {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Typst Editor".to_owned()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let editing = *self.edit_in_progress.val_or_else(|| false);

        let avail_w = ctx.constraints.x.map(|a| a.provided_value()).unwrap_or(0.0);
        let avail_h = ctx.constraints.y.map(|a| a.provided_value()).unwrap_or(0.0);
        let origin = ctx.constraints.pos;

        let preview = self.rendered(&ctx);
        let editor_id = egui::Id::new(self.editor.erase());

        if !editing {
            // Show the rendered document filling the whole area.
            let constraints = ctx.constraints;
            let res = ctx.draw_node(&*preview, constraints);

            ctx.draw_workspace_node(self.click_sensor.erase(), constraints);
            if ctx
                .node
                .workspace
                .send_request(self.click_sensor, WasClicked {})
                .unwrap_or(false)
            {
                self.edit_in_progress.set(true);
            }

            res
        } else {
            const GAP: f32 = theme::SPACE_MD;
            let preview_height = avail_h * 0.4;

            ctx.overlay_node(
                &*preview,
                DrawConstraints {
                    pos: ScreenPos {
                        x: origin.x,
                        y: origin.y - GAP - preview_height,
                    },
                    x: Some(AxisConstraint::AtMost(avail_w)),
                    y: Some(AxisConstraint::AtMost(preview_height)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
            );

            let editor_res = ctx.draw_workspace_node(self.editor.erase(), ctx.constraints);
            let editor_region = editor_res
                .and_then(|res| res.region())
                .unwrap_or(ScreenRegion::empty());

            let (has_focus, had_focus) = ctx
                .ui
                .memory_mut(|m| (m.has_focus(editor_id), m.had_focus_last_frame(editor_id)));
            if had_focus && !has_focus {
                self.edit_in_progress.set(false);
            } else if !has_focus && !had_focus {
                ctx.ui.memory_mut(|m| m.request_focus(editor_id));
            }

            DrawResult::Complete {
                region: Some(editor_region),
            }
        }
    }
}

defhandlers! { TypstEditor {} }
