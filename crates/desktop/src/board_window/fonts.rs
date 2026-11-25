use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};

pub(super) fn install(ctx: &Context, custom: Option<std::sync::Arc<FontData>>) {
    let mut fonts = FontDefinitions::default();
    for (family, name, bytes) in [
        (
            FontFamily::Proportional,
            "Barlow",
            include_bytes!("../../assets/fonts/Barlow-Regular.ttf").as_slice(),
        ),
        (
            FontFamily::Monospace,
            "Courier Prime",
            include_bytes!("../../assets/fonts/CourierPrime-Regular.ttf").as_slice(),
        ),
        (
            FontFamily::Name("Marker".into()),
            "Permanent Marker",
            include_bytes!("../../assets/fonts/PermanentMarker-Regular.ttf").as_slice(),
        ),
        (
            FontFamily::Name("Placard".into()),
            "Anton",
            include_bytes!("../../assets/fonts/Anton-Regular.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), FontData::from_static(bytes).into());
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, name.into());
    }
    let mut titles = vec!["Permanent Marker".to_owned(), "Barlow".to_owned()];
    if let Some(data) = custom {
        fonts.font_data.insert("Board font".into(), data);
        titles.insert(0, "Board font".into());
    }
    fonts
        .families
        .insert(FontFamily::Name("Custom title".into()), titles);
    ctx.set_fonts(fonts);
}
