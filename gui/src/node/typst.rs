use crate::{
    node::{
        DrawContext, LayoutContext, NodeDynamics, NodeInitialization, NodeVariant,
        view::{ResizeDir, Window},
    },
    prelude::*,
};

use egui::{Id, Image, ImageSource, TextStyle, TextureOptions, UiBuilder};
use egui_code_editor::{CodeEditor, ColorTheme};
use typst::{
    Library, LibraryExt,
    diag::{FileError, FileResult, SourceDiagnostic},
    ecow::EcoVec,
    foundations::Bytes,
    syntax::{FileId, Source, VirtualPath},
    text::{Font, FontBook},
    utils::LazyHash,
};

#[derive(Clone)]
pub struct IncrementalTypstWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    source: Source,
}

fn fonts() -> Vec<Font> {
    let mut all_fonts = Vec::new();

    let all_font_data: [&'static [u8]; _] = [
        include_bytes!("../../assets/Inter_Regular.ttf"),
        include_bytes!("../../assets/LibertinusMath.ttf"),
    ];

    for font_data in all_font_data {
        let buffer = Bytes::new(font_data);
        for font in Font::iter(buffer) {
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
            source: Source::new(FileId::new_fake(VirtualPath::new("<input>")), String::new()),
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
    #set text(font: "Inter")
    #show math.equation: set text(font: "Libertinus Math")
            "#;
        let full_code = format!("{}\n{}", header.trim(), code);

        self.update_source(&full_code);
        let document: typst::layout::PagedDocument = typst::compile(self).output?;
        let img = typst_svg::svg(&document.pages[0]);

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

    fn font(&self, id: usize) -> Option<Font> {
        self.fonts.get(id).cloned()
    }

    fn file(&self, path: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(path.vpath().as_rootless_path().into()))
    }

    fn today(&self, _offset: Option<i64>) -> Option<typst::foundations::Datetime> {
        None
    }
}

#[derive(Clone)]
pub struct TypstPayload {
    code: Buffer<String>,
    world: Transient<IncrementalTypstWorld>,
    image: Transient<Vec<u8>>,
    last_err: Transient<Option<String>>,
    view: Window,
}

impl NodeInitialization for TypstPayload {
    type Origin = String;

    fn init_from(f: Self::Origin, seed: u32) -> Self {
        let mut world = IncrementalTypstWorld::default();
        let code = Buffer::new(f, Id::new(seed).with("code_buffer"));
        let empty_image = Transient::from(world.render("").unwrap().as_bytes().to_vec());

        Self {
            code,
            world: Transient::from(world),
            image: empty_image,
            last_err: Transient::from(None),
            view: Window::default(),
        }
    }
}

impl NodeDynamics for TypstPayload {
    fn step(&self, ctx: &mut DrawContext<'_>) {
        action! {
            SetTypstCode { idx: NodeIdx, val: String }
                does(ctx) {
                    let typst_node = ctx.unwrap_mut_with(idx, NodeVariant::try_as_typst_mut);
                    typst_node.code.set(val);
                }
        }

        self.code
            .resolve_pending_actions(ctx.ui, ctx.action_queue, |s| SetTypstCode {
                idx: ctx.index,
                val: s,
            });
    }

    fn draw(&self, ctx: &mut DrawContext<'_>) {
        let image = self.image.clone();
        let node_id = ctx.id.value();

        self.view.show(
            ctx,
            Color32::TRANSPARENT,
            |ui, _actions| {
                let uri = format!("bytes://typst_{}.svg", node_id);
                // Evict cache
                ui.ctx().forget_image(&uri);
                ui.vertical_centered(|ui| {
                    ui.add(
                        Image::new(ImageSource::Bytes {
                            uri: uri.into(),
                            bytes: (*image.val()).clone().into(),
                        })
                        .texture_options(TextureOptions::NEAREST)
                        .fit_to_original_size(1.5),
                    );
                });
            },
            |ui, _actions| {
                if let Some(err) = &*self.last_err.val() {
                    ui.colored_label(Color32::RED, err);
                }
                self.last_err.set(None);

                self.code.show(|code, id| {
                    ui.add_sized(ui.available_size(), |ui: &mut Ui| {
                        CodeEditor::default()
                            .id(id)
                            .with_theme(ColorTheme::AYU)
                            .with_fontsize(ui.text_style_height(&TextStyle::Monospace))
                            .show(ui, code)
                            .response
                            .response
                    })
                });

                let compile_output = self
                    .world
                    .modify(|world| world.render(&self.code.temp_str()));
                match compile_output {
                    Ok(svg) => self.image.set(svg.as_bytes().to_vec()),
                    Err(e) => {
                        let e_string = format!("{e:?}");
                        if self.last_err.val().as_deref() != Some(&e_string) {
                            self.last_err.set(Some(e_string));
                        }
                    }
                }
            },
        );
    }

    fn resize(&mut self, dir: ResizeDir, delta: Vec2) {
        self.view.handle_resize(dir, delta);
    }

    fn size(&self, _ctx: LayoutContext) -> Vec2 {
        self.view.sizes().1
    }
}
