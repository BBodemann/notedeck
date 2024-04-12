use egui::Color32;

pub const PURPLE: Color32 = Color32::from_rgb(0xCC, 0x43, 0xC5);
//pub const DARK_BG: Color32 = egui::Color32::from_rgb(40, 44, 52);
const GRAY_SECONDARY: Color32 = Color32::from_rgb(0x8A, 0x8A, 0x8A);
const WHITE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
const BLACK: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
const RED_700: Color32 = Color32::from_rgb(0xC7, 0x37, 0x5A);

// BACKGROUNDS
const SEMI_DARKER_BG: Color32 = Color32::from_rgb(0x39, 0x39, 0x39);
const DARKER_BG: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x1E);
const DARK_BG: Color32 = Color32::from_rgb(0x2C, 0x2C, 0x2C);
const DARK_ISH_BG: Color32 = Color32::from_rgb(0x22, 0x22, 0x22);
const SEMI_DARK_BG: Color32 = Color32::from_rgb(0x44, 0x44, 0x44);

const LIGHT_GRAY: Color32 = Color32::from_rgb(0xc8, 0xc8, 0xc8); // 78%
const MID_GRAY: Color32 = Color32::from_rgb(0xba, 0xba, 0xba); // 72%
const DARKER_GRAY: Color32 = Color32::from_rgb(0xa5, 0xa5, 0xa5); // 65%
const EVEN_DARKER_GRAY: Color32 = Color32::from_rgb(0x89, 0x89, 0x89); // 54%

pub struct ColorTheme {
    // VISUALS
    pub panel_fill: Color32,
    pub extreme_bg_color: Color32,
    pub text_color: Color32,
    pub err_fg_color: Color32,
    pub hyperlink_color: Color32,

    // WINDOW
    pub window_fill: Color32,
    pub window_stroke_color: Color32,

    // NONINTERACTIVE WIDGET
    pub noninteractive_bg_fill: Color32,
    pub noninteractive_weak_bg_fill: Color32,
    pub noninteractive_bg_stroke_color: Color32,
    pub noninteractive_fg_stroke_color: Color32,

    // INACTIVE WIDGET
    pub inactive_bg_stroke_color: Color32,
    pub inactive_bg_fill: Color32,
    pub inactive_weak_bg_fill: Color32,
}

pub struct DarkTheme;
pub struct LightTheme;

pub fn dark_color_theme() -> ColorTheme {
    ColorTheme {
        // VISUALS
        panel_fill: DARKER_BG,
        extreme_bg_color: SEMI_DARKER_BG,
        text_color: WHITE,
        err_fg_color: RED_700,
        hyperlink_color: PURPLE,

        // WINDOW
        window_fill: DARK_ISH_BG,
        window_stroke_color: DARK_BG,

        // NONINTERACTIVE WIDGET
        noninteractive_bg_fill: DARK_ISH_BG,
        noninteractive_weak_bg_fill: SEMI_DARKER_BG,
        noninteractive_bg_stroke_color: DARK_BG,
        noninteractive_fg_stroke_color: GRAY_SECONDARY,

        // INACTIVE WIDGET
        inactive_bg_stroke_color: SEMI_DARKER_BG,
        inactive_bg_fill: Color32::from_rgb(0x25, 0x25, 0x25),
        inactive_weak_bg_fill: SEMI_DARK_BG,
    }
}

pub fn light_color_theme() -> ColorTheme {
    ColorTheme {
        // VISUALS
        panel_fill: LIGHT_GRAY,
        extreme_bg_color: EVEN_DARKER_GRAY,
        text_color: BLACK,
        err_fg_color: RED_700,
        hyperlink_color: PURPLE,

        // WINDOW
        window_fill: MID_GRAY,
        window_stroke_color: DARKER_GRAY,

        // NONINTERACTIVE WIDGET
        noninteractive_bg_fill: MID_GRAY,
        noninteractive_weak_bg_fill: EVEN_DARKER_GRAY,
        noninteractive_bg_stroke_color: DARKER_GRAY,
        noninteractive_fg_stroke_color: GRAY_SECONDARY,

        // INACTIVE WIDGET
        inactive_bg_stroke_color: EVEN_DARKER_GRAY,
        inactive_bg_fill: LIGHT_GRAY,
        inactive_weak_bg_fill: EVEN_DARKER_GRAY,
    }
}
