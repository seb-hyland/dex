use egui::{Context, FontData, FontDefinitions, FontFamily};

pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();

    let faces: [(&str, &'static [u8]); 4] = [
        (
            "Literata",
            include_bytes!("../../../assets/Literata/Literata-Regular.ttf"),
        ),
        (
            "Literata-Bold",
            include_bytes!("../../../assets/Literata/Literata-Bold.ttf"),
        ),
        (
            "Literata-Italic",
            include_bytes!("../../../assets/Literata/Literata-Italic.ttf"),
        ),
        (
            "Literata-BoldItalic",
            include_bytes!("../../../assets/Literata/Literata-BoldItalic.ttf"),
        ),
    ];
    for (name, bytes) in faces {
        fonts
            .font_data
            .insert(name.to_owned(), FontData::from_static(bytes).into());
    }

    // Fallbacks after.
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Literata".to_owned());

    // Named families so callers can select a specific weight/style.
    for name in ["Literata-Bold", "Literata-Italic", "Literata-BoldItalic"] {
        fonts
            .families
            .insert(FontFamily::Name(name.into()), vec![name.to_owned()]);
    }

    ctx.set_fonts(fonts);
}
