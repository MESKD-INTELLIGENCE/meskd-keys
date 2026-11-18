use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Visuals};

pub const BG: Color32 = Color32::from_rgb(0x10, 0x12, 0x16);
pub const CARD: Color32 = Color32::from_rgb(0x18, 0x1b, 0x21);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x1e, 0x22, 0x29);
pub const LINE: Color32 = Color32::from_rgb(0x2a, 0x2f, 0x38);
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe8, 0xeb);
pub const MUTED: Color32 = Color32::from_rgb(0x8b, 0x93, 0x9e);
pub const ACCENT: Color32 = Color32::from_rgb(0x3d, 0xd6, 0x9a);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x1f, 0x4d, 0x3c);
pub const WARN: Color32 = Color32::from_rgb(0xf0, 0xa2, 0x3b);
pub const DANGER: Color32 = Color32::from_rgb(0xef, 0x5b, 0x5b);

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (
                TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(14.5, FontFamily::Proportional),
            ),
            (
                TextStyle::Heading,
                FontId::new(20.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(14.0, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.text_edit_width = 260.0;

        let mut v = Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = CARD;
        v.extreme_bg_color = Color32::from_rgb(0x0b, 0x0d, 0x10);
        v.faint_bg_color = CARD;
        v.window_stroke = Stroke::new(1.0, LINE);
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(8);
        v.selection.bg_fill = ACCENT_DIM;
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.hyperlink_color = ACCENT;
        v.override_text_color = Some(TEXT);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = CornerRadius::same(8);
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x22, 0x26, 0x2e);
        v.widgets.inactive.bg_fill = Color32::from_rgb(0x22, 0x26, 0x2e);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x2a, 0x2f, 0x38);
        v.widgets.hovered.bg_fill = Color32::from_rgb(0x2a, 0x2f, 0x38);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x3a, 0x40, 0x4b));
        v.widgets.active.weak_bg_fill = ACCENT_DIM;
        v.widgets.active.bg_fill = ACCENT_DIM;
        style.visuals = v;
    });
}

pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(14, 10))
}

pub fn dialog() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(20))
}

pub fn primary(text: &str) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text.to_owned())
            .color(Color32::from_rgb(0x07, 0x1a, 0x12))
            .strong(),
    )
    .fill(ACCENT)
}

pub fn danger(text: &str) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text.to_owned())
            .color(Color32::WHITE)
            .strong(),
    )
    .fill(DANGER)
}

const ICON_PNG: &[u8] = include_bytes!("../../assets/icon-256.png");

pub fn icon() -> egui::IconData {
    match image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png) {
        Ok(img) => {
            let img = img.to_rgba8();
            egui::IconData {
                width: img.width(),
                height: img.height(),
                rgba: img.into_raw(),
            }
        }
        Err(_) => egui::IconData::default(),
    }
}

pub fn logo(ui: &mut egui::Ui, size: f32) {
    let texture = ui.ctx().memory_mut(|m| {
        m.data
            .get_temp::<egui::TextureHandle>(egui::Id::unique("logo"))
    });
    let texture = texture.unwrap_or_else(|| {
        let icon = icon();
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [icon.width as usize, icon.height as usize],
            &icon.rgba,
        );
        let handle = ui
            .ctx()
            .load_texture("logo", image, egui::TextureOptions::LINEAR);
        ui.ctx()
            .memory_mut(|m| m.data.insert_temp(egui::Id::unique("logo"), handle.clone()));
        handle
    });
    ui.add(egui::Image::new((texture.id(), egui::vec2(size, size))));
}

pub fn name(response: &egui::Response, label: &str) {
    let label = label.to_owned();
    response
        .ctx
        .accesskit_node_builder(response.id, |node| node.set_label(label));
}
