//! Central UI theme. Every screen pulls colours, sizes and fonts from
//! [`UiTheme`] through the semantic style helpers below — no screen hardcodes
//! its own palette.

use bevy::prelude::*;

#[derive(Resource, Clone)]
pub struct UiTheme {
    pub bg: Color,
    pub panel: Color,
    pub panel_border: Color,
    pub danger: Color,
    pub interact: Color,
    pub text: Color,
    pub text_dim: Color,
    pub button_bg: Color,
    pub button_bg_focus: Color,
    pub button_bg_pressed: Color,
    pub font_regular: Handle<Font>,
    pub font_bold: Handle<Font>,
    pub size_title: f32,
    pub size_heading: f32,
    pub size_body: f32,
    pub size_small: f32,
    pub pad: f32,
    pub gap: f32,
    pub radius: f32,
    pub border: f32,
}

pub const fn hex(rgb: u32) -> Color {
    Color::srgb(
        ((rgb >> 16) & 0xFF) as f32 / 255.0,
        ((rgb >> 8) & 0xFF) as f32 / 255.0,
        (rgb & 0xFF) as f32 / 255.0,
    )
}

impl UiTheme {
    pub fn new(asset_server: &AssetServer) -> Self {
        Self {
            bg: hex(0x0B0E14),
            panel: hex(0x151A24),
            panel_border: hex(0x262E3D),
            danger: hex(0xE05A4E),
            interact: hex(0x7FD1D9),
            text: hex(0xE8E6E3),
            text_dim: hex(0x8A8F98),
            button_bg: hex(0x1A202C),
            button_bg_focus: hex(0x1F2C36),
            button_bg_pressed: hex(0x2A3E48),
            font_regular: asset_server.load("fonts/IBMPlexMono-Regular.ttf"),
            font_bold: asset_server.load("fonts/IBMPlexMono-Bold.ttf"),
            // Sizes are in logical pixels; the UI scale system keeps them
            // readable from 1366x768 up to 1920x1080 and beyond.
            size_title: 88.0,
            size_heading: 34.0,
            size_body: 22.0,
            size_small: 17.0,
            pad: 24.0,
            gap: 12.0,
            radius: 6.0,
            border: 2.0,
        }
    }

    pub fn font(&self, bold: bool, size: f32) -> TextFont {
        TextFont {
            font: if bold { self.font_bold.clone() } else { self.font_regular.clone() }.into(),
            font_size: FontSize::Px(size),
            ..default()
        }
    }

    // ---- Semantic text styles -------------------------------------------

    pub fn title_text(&self, s: impl Into<String>) -> impl Bundle {
        (Text::new(s), self.font(true, self.size_title), TextColor(self.text))
    }

    pub fn heading_text(&self, s: impl Into<String>) -> impl Bundle {
        (Text::new(s), self.font(true, self.size_heading), TextColor(self.text))
    }

    pub fn body_text(&self, s: impl Into<String>) -> impl Bundle {
        (Text::new(s), self.font(false, self.size_body), TextColor(self.text))
    }

    pub fn dim_text(&self, s: impl Into<String>) -> impl Bundle {
        (Text::new(s), self.font(false, self.size_small), TextColor(self.text_dim))
    }

    pub fn hud_text(&self, s: impl Into<String>) -> impl Bundle {
        self.hud_text_sized(s, true, self.size_body)
    }

    pub fn hud_text_sized(&self, s: impl Into<String>, bold: bool, size: f32) -> impl Bundle {
        (
            Text::new(s),
            self.font(bold, size),
            TextColor(self.text),
            TextShadow { offset: Vec2::splat(2.0), color: Color::BLACK.with_alpha(0.8) },
        )
    }

    // ---- Semantic containers --------------------------------------------

    /// Full-screen opaque backdrop used by menus.
    pub fn screen_root(&self, opaque: bool) -> impl Bundle {
        (
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(self.gap),
                ..default()
            },
            BackgroundColor(if opaque { self.bg } else { self.bg.with_alpha(0.82) }),
        )
    }

    pub fn panel(&self, min_width: f32) -> impl Bundle {
        (
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                min_width: px(min_width),
                padding: UiRect::all(px(self.pad)),
                row_gap: px(self.gap),
                border: UiRect::all(px(self.border)),
                border_radius: BorderRadius::all(px(self.radius)),
                ..default()
            },
            BackgroundColor(self.panel),
            BorderColor::all(self.panel_border),
        )
    }

    pub fn danger_panel(&self, min_width: f32) -> impl Bundle {
        (
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                min_width: px(min_width),
                padding: UiRect::all(px(self.pad)),
                row_gap: px(self.gap),
                border: UiRect::all(px(self.border)),
                border_radius: BorderRadius::all(px(self.radius)),
                ..default()
            },
            BackgroundColor(self.panel),
            BorderColor::all(self.danger),
        )
    }

    /// Node for a primary menu button (text child added by the caller).
    pub fn primary_button(&self) -> impl Bundle {
        (
            Node {
                min_width: px(320),
                height: px(54),
                padding: UiRect::horizontal(px(20)),
                border: UiRect::all(px(self.border)),
                border_radius: BorderRadius::all(px(self.radius)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(self.button_bg),
            BorderColor::all(self.panel_border),
        )
    }

    /// A settings row: label on the left, value on the right.
    pub fn row(&self) -> impl Bundle {
        (
            Node {
                min_width: px(560),
                height: px(46),
                padding: UiRect::horizontal(px(16)),
                border: UiRect::all(px(self.border)),
                border_radius: BorderRadius::all(px(self.radius)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: px(16),
                ..default()
            },
            BackgroundColor(self.button_bg),
            BorderColor::all(self.panel_border),
        )
    }
}

pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, scale_ui);
    }

    // `finish` runs after every plugin is built, so the AssetServer exists and
    // the theme is ready before the first state's OnEnter systems spawn UI.
    fn finish(&self, app: &mut App) {
        let theme = UiTheme::new(app.world().resource::<AssetServer>());
        app.insert_resource(theme);
    }
}

/// Keeps layouts authored at 1920x1080 readable on smaller windows (down to
/// 1366x768) without letting text become tiny: scale tracks window height
/// but never drops below 0.72.
fn scale_ui(windows: Query<&Window, With<bevy::window::PrimaryWindow>>, mut scale: ResMut<UiScale>) {
    let Ok(window) = windows.single() else { return };
    let h = window.height().max(1.0);
    let target = (h / 1080.0).clamp(0.72, 1.6);
    if (scale.0 - target).abs() > 0.01 {
        scale.0 = target;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_matches_palette() {
        let c = hex(0x0B0E14).to_srgba();
        assert!((c.red - 11.0 / 255.0).abs() < 1e-6);
        assert!((c.green - 14.0 / 255.0).abs() < 1e-6);
        assert!((c.blue - 20.0 / 255.0).abs() < 1e-6);
    }
}
