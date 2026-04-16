use crate::{
    node::{DrawContext, LayoutContext, NodeDynamics, view::Window},
    prelude::*,
};

use egui::{Image, ImageSource, TextStyle, TextureOptions};
use egui_code_editor::{CodeEditor, ColorTheme};
use time::{OffsetDateTime, UtcOffset};
use typst::{
    Library, LibraryExt,
    diag::{FileError, FileResult, SourceDiagnostic},
    ecow::EcoVec,
    foundations::{Bytes, Datetime},
    syntax::{FileId, Source, VirtualPath},
    text::{Font, FontBook},
    utils::LazyHash,
};

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

    fn today(&self, offset: Option<i64>) -> Option<typst::foundations::Datetime> {
        let mut cur_time =
            OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        if let Some(offset) = offset {
            cur_time = cur_time.to_offset(UtcOffset::from_hms(offset as i8, 0, 0).unwrap());
        }

        let (h, m, s) = cur_time.to_hms();
        Datetime::from_hms(h, m, s)
    }
}

#[derive(Serialize, Deserialize)]
pub struct TypstPayload {
    code: String,
    #[serde(skip)]
    world: IncrementalTypstWorld,
    image: Vec<u8>,
    last_err: Option<String>,
    view: Window,
}

impl Default for TypstPayload {
    fn default() -> Self {
        let mut world = IncrementalTypstWorld::default();
        let code = String::new();
        let image = world.render(&code).unwrap().as_bytes().into();

        Self {
            code,
            world,
            image,
            last_err: None,
            view: Window::default(),
        }
    }
}

impl NodeDynamics for TypstPayload {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) {
        let image = self.image.clone();
        let node_id = ctx.id.value();

        self.view.show(
            ctx,
            Color32::TRANSPARENT,
            |ui| {
                let uri = format!("bytes://typst_{}.svg", node_id);
                // Evict cache
                ui.ctx().forget_image(&uri);
                ui.vertical_centered(|ui| {
                    ui.add(
                        Image::new(ImageSource::Bytes {
                            uri: uri.into(),
                            bytes: image.into(),
                        })
                        .texture_options(TextureOptions::NEAREST)
                        .fit_to_original_size(1.5),
                    );
                });
            },
            |ui| {
                if let Some(err) = &self.last_err {
                    ui.colored_label(Color32::RED, err);
                }
                self.last_err = None;

                ui.add_sized(ui.available_size(), |ui: &mut Ui| {
                    CodeEditor::default()
                        .id_source(ui.id().with("code_editor").value().to_string())
                        .with_theme(ColorTheme::AYU)
                        .with_fontsize(ui.text_style_height(&TextStyle::Monospace))
                        .show(ui, &mut self.code)
                        .response
                        .response
                });

                let compile_output = self.world.render(&self.code);
                match compile_output {
                    Ok(svg) => self.image = svg.as_bytes().to_vec(),
                    Err(e) => self.last_err = Some(format!("{e:?}")),
                }
            },
        );
    }

    fn size(&self, _ctx: LayoutContext) -> Vec2 {
        self.view.sizes().1
    }
}
