use dex_core::prelude::{BOLD_FAMILY, BOLD_ITALIC_FAMILY, ITALIC_FAMILY};
use egui::{Context, FontData, FontDefinitions, FontFamily};

pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();

    let faces: [(&str, &'static [u8]); 4] = [
        (
            "Literata",
            include_bytes!("../../../assets/Literata/Literata-Regular.ttf"),
        ),
        (
            BOLD_FAMILY,
            include_bytes!("../../../assets/Literata/Literata-Bold.ttf"),
        ),
        (
            ITALIC_FAMILY,
            include_bytes!("../../../assets/Literata/Literata-Italic.ttf"),
        ),
        (
            BOLD_ITALIC_FAMILY,
            include_bytes!("../../../assets/Literata/Literata-BoldItalic.ttf"),
        ),
    ];
    for (name, bytes) in faces {
        fonts
            .font_data
            .insert(name.to_owned(), FontData::from_static(bytes).into());
    }

    // Fallbacks after.
    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, "Literata".to_owned());
    let fallbacks = proportional.clone();

    for name in [BOLD_FAMILY, ITALIC_FAMILY, BOLD_ITALIC_FAMILY] {
        let mut chain = vec![name.to_owned()];
        chain.extend(fallbacks.iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), chain);
    }

    ctx.set_fonts(fonts);
}
