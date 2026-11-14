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
            ScreenRoot,
        )
    }

    /// A card that scrolls vertically when its contents are taller than the
    /// screen allows (mouse wheel, touch drag, or keyboard focus).
    pub fn panel(&self, min_width: f32) -> impl Bundle {
        (
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                min_width: px(min_width),
                // QA: HALLWAYS_PANEL_VH squeezes panels to test scrolling.
                max_height: vh(std::env::var("HALLWAYS_PANEL_VH").ok().and_then(|v| v.parse().ok()).unwrap_or(64.0)),
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                padding: UiRect::all(px(self.pad)),
                row_gap: px(self.gap),
                border: UiRect::all(px(self.border)),
                border_radius: BorderRadius::all(px(self.radius)),
                ..default()
            },
            BackgroundColor(self.panel),
            BorderColor::all(self.panel_border),
            ScrollPosition::default(),
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

/// Marks a full-screen menu; the UI scale shrinks until it fits the window.
#[derive(Component)]
pub struct ScreenRoot;

/// Keeps layouts authored at 1920x1080 readable on smaller windows (down to
/// 1366x768) without letting text become tiny, then shrinks further while an
/// open menu is wider or taller than the window (phones) and grows back once
/// there is room again.
fn scale_ui(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    roots: Query<(&ComputedNode, &InheritedVisibility, &Children), With<ScreenRoot>>,
    nodes: Query<(&ComputedNode, &Node, &bevy::ui::UiGlobalTransform)>,
    mut scale: ResMut<UiScale>,
) {
    let Ok(window) = windows.single() else { return };
    let base = ui_scale_for(window.width(), window.height());
    // How much of the content fits: < 1 means it spills off screen.
    let mut ratio = f32::INFINITY;
    for (root, vis, kids) in &roots {
        if !vis.get() || root.size.x < 1.0 {
            continue;
        }
        let (w, h) = content_extent(kids, &nodes);
        let inset = root.content_inset();
        let room = root.size - inset.min_inset - inset.max_inset;
        if std::env::var_os("HALLWAYS_UI_DEBUG").is_some() {
            info!("UI fit: room {room:?} content {w}x{h} scale {}", scale.0);
        }
        // Content that exactly fills the width is stretched to it (e.g. menu
        // buttons): it says nothing about fitting. Anything else counts.
        if w > 1.0 && (w - room.x).abs() > 1.0 {
            ratio = ratio.min(room.x / w);
        }
        if h > 1.0 {
            ratio = ratio.min(room.y / h);
        }
    }
    let target = fit_scale(scale.0, base, ratio);
    if (scale.0 - target).abs() > 0.005 {
        scale.0 = target;
    }
}

/// Width and height spanned by a menu's laid-out children (physical pixels),
/// gaps and margins included. Absolutely placed children don't count.
fn content_extent(kids: &Children, nodes: &Query<(&ComputedNode, &Node, &bevy::ui::UiGlobalTransform)>) -> (f32, f32) {
    let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
    for k in kids.iter() {
        let Ok((c, node, tf)) = nodes.get(k) else { continue };
        if node.position_type == PositionType::Absolute || c.size.y < 0.5 {
            continue;
        }
        lo = lo.min(tf.translation - c.size / 2.0);
        hi = hi.max(tf.translation + c.size / 2.0);
    }
    if lo.x > hi.x {
        return (0.0, 0.0);
    }
    let span = hi - lo;
    // A little breathing room top and bottom. Width gets none: buttons that
    // stretch to fill the menu exactly must not count as overflowing.
    (span.x, span.y * 1.03)
}

/// Next UI scale: never above `base`; shrink when content doesn't fit
/// (`ratio` < 1), grow back toward `base` when there is clear room.
pub fn fit_scale(current: f32, base: f32, ratio: f32) -> f32 {
    let target = if !ratio.is_finite() {
        base
    } else if ratio < 0.995 || ratio > 1.08 {
        current * ratio * 0.98
    } else {
        current
    };
    target.clamp(0.3, base)
}

/// Base UI scale for a window of `w` x `h` logical pixels (before fitting).
pub fn ui_scale_for(_w: f32, h: f32) -> f32 {
    (h.max(1.0) / 1080.0).clamp(0.72, 1.6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_scale_fits_then_grows_back() {
        // Desktop sizes keep the old behaviour when everything fits.
        assert!((ui_scale_for(1920.0, 1080.0) - 1.0).abs() < 1e-3);
        assert!((ui_scale_for(1366.0, 768.0) - 0.72).abs() < 1e-3);
        assert_eq!(fit_scale(0.72, 0.72, f32::INFINITY), 0.72);
        assert_eq!(fit_scale(0.72, 0.72, 1.02), 0.72, "small slack: no change");
        // Content 1.6x too wide: shrink so it fits.
        let k = fit_scale(0.72, 0.72, 1.0 / 1.6);
        assert!(k < 0.72 / 1.6 + 1e-3 && k > 0.4);
        // Room again (menu closed or smaller): back up, never past the base.
        assert_eq!(fit_scale(k, 0.72, 3.0), 0.72);
        assert_eq!(fit_scale(k, 0.72, f32::INFINITY), 0.72);
    }

    #[test]
    fn hex_matches_palette() {
        let c = hex(0x0B0E14).to_srgba();
        assert!((c.red - 11.0 / 255.0).abs() < 1e-6);
        assert!((c.green - 14.0 / 255.0).abs() < 1e-6);
        assert!((c.blue - 20.0 / 255.0).abs() < 1e-6);
    }
}
