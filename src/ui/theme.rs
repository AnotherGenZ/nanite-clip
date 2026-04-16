//! Design tokens for the nanite-ui component library.
//!
//! The token layer ships three preset families (`Zinc`, `Slate`, `Nanite`)
//! each with a light and dark mode, giving six complete token sets. Space,
//! radius, and font scales are shared across every preset, while colour and
//! shadow tokens vary.
//!
//! `preset.iced_theme(mode)` builds an `iced::Theme::Custom` with a distinct
//! name so [`tokens_for`] can round-trip any `iced::Theme` reference inside a
//! primitive style closure back to the matching token set.

use iced::theme::{Base, Palette, palette};
use iced::{Color, Shadow, Theme, Vector, border};

/// Full token set for one preset+mode combination.
#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub color: ColorTokens,
    pub space: SpaceTokens,
    pub radius: RadiusTokens,
    pub font: FontTokens,
    pub shadow: ShadowTokens,
}

/// Semantic color roles. Naming mirrors shadcn/tailwind so templates port
/// over directly; `*_hover` / `*_active` are our own extension to keep
/// primitives stateful without relying on alpha mixing at draw time.
#[derive(Debug, Clone, Copy)]
pub struct ColorTokens {
    pub background: Color,
    pub foreground: Color,
    pub card: Color,
    pub card_foreground: Color,
    pub popover: Color,
    pub popover_foreground: Color,
    pub primary: Color,
    pub primary_hover: Color,
    pub primary_active: Color,
    pub primary_foreground: Color,
    pub secondary: Color,
    pub secondary_hover: Color,
    pub secondary_active: Color,
    pub secondary_foreground: Color,
    pub muted: Color,
    pub muted_foreground: Color,
    pub accent: Color,
    pub accent_foreground: Color,
    pub destructive: Color,
    pub destructive_hover: Color,
    pub destructive_active: Color,
    pub destructive_foreground: Color,
    pub warning: Color,
    pub warning_foreground: Color,
    pub success: Color,
    pub success_foreground: Color,
    pub info: Color,
    pub info_foreground: Color,
    pub border: Color,
    pub border_strong: Color,
    pub input: Color,
    pub input_foreground: Color,
    pub ring: Color,
    pub overlay: Color,
    pub skeleton: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct SpaceTokens {
    pub xxs: f32,
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xxl: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct RadiusTokens {
    pub none: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub full: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct FontTokens {
    pub size_xs: f32,
    pub size_sm: f32,
    pub size_base: f32,
    pub size_lg: f32,
    pub size_xl: f32,
    pub size_2xl: f32,
    pub size_3xl: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct ShadowTokens {
    pub sm: Shadow,
    pub md: Shadow,
    pub lg: Shadow,
    pub xl: Shadow,
}

// ---------- non-color tokens (shared across presets) ------------------------

pub const SPACE: SpaceTokens = SpaceTokens {
    xxs: 2.0,
    xs: 4.0,
    sm: 6.0,
    md: 10.0,
    lg: 14.0,
    xl: 20.0,
    xxl: 28.0,
};

pub const RADIUS: RadiusTokens = RadiusTokens {
    none: 0.0,
    sm: 4.0,
    md: 6.0,
    lg: 10.0,
    xl: 14.0,
    full: 9999.0,
};

pub const FONT: FontTokens = FontTokens {
    size_xs: 11.0,
    size_sm: 12.0,
    size_base: 14.0,
    size_lg: 16.0,
    size_xl: 18.0,
    size_2xl: 22.0,
    size_3xl: 28.0,
};

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}

const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::from_rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

const DARK_SHADOWS: ShadowTokens = ShadowTokens {
    sm: Shadow {
        color: rgba(0, 0, 0, 0.35),
        offset: Vector::new(0.0, 1.0),
        blur_radius: 2.0,
    },
    md: Shadow {
        color: rgba(0, 0, 0, 0.45),
        offset: Vector::new(0.0, 4.0),
        blur_radius: 10.0,
    },
    lg: Shadow {
        color: rgba(0, 0, 0, 0.55),
        offset: Vector::new(0.0, 10.0),
        blur_radius: 22.0,
    },
    xl: Shadow {
        color: rgba(0, 0, 0, 0.65),
        offset: Vector::new(0.0, 18.0),
        blur_radius: 36.0,
    },
};

const LIGHT_SHADOWS: ShadowTokens = ShadowTokens {
    sm: Shadow {
        color: rgba(15, 20, 30, 0.08),
        offset: Vector::new(0.0, 1.0),
        blur_radius: 2.0,
    },
    md: Shadow {
        color: rgba(15, 20, 30, 0.12),
        offset: Vector::new(0.0, 4.0),
        blur_radius: 10.0,
    },
    lg: Shadow {
        color: rgba(15, 20, 30, 0.16),
        offset: Vector::new(0.0, 10.0),
        blur_radius: 22.0,
    },
    xl: Shadow {
        color: rgba(15, 20, 30, 0.22),
        offset: Vector::new(0.0, 18.0),
        blur_radius: 36.0,
    },
};

// ---------- Zinc (stark neutral — the shadcn default look) -----------------

pub const ZINC_DARK: Tokens = Tokens {
    color: ColorTokens {
        background: rgb(9, 9, 11),
        foreground: rgb(250, 250, 250),
        card: rgb(9, 9, 11),
        card_foreground: rgb(250, 250, 250),
        popover: rgb(9, 9, 11),
        popover_foreground: rgb(250, 250, 250),
        primary: rgb(250, 250, 250),
        primary_hover: rgb(228, 228, 231),
        primary_active: rgb(212, 212, 216),
        primary_foreground: rgb(24, 24, 27),
        secondary: rgb(39, 39, 42),
        secondary_hover: rgb(52, 52, 56),
        secondary_active: rgb(63, 63, 70),
        secondary_foreground: rgb(250, 250, 250),
        muted: rgb(39, 39, 42),
        muted_foreground: rgb(161, 161, 170),
        accent: rgb(39, 39, 42),
        accent_foreground: rgb(250, 250, 250),
        destructive: rgb(220, 38, 38),
        destructive_hover: rgb(239, 68, 68),
        destructive_active: rgb(185, 28, 28),
        destructive_foreground: rgb(250, 250, 250),
        warning: rgb(234, 179, 8),
        warning_foreground: rgb(24, 16, 0),
        success: rgb(34, 197, 94),
        success_foreground: rgb(6, 32, 16),
        info: rgb(56, 189, 248),
        info_foreground: rgb(5, 22, 30),
        border: rgb(39, 39, 42),
        border_strong: rgb(63, 63, 70),
        input: rgb(39, 39, 42),
        input_foreground: rgb(250, 250, 250),
        ring: rgba(212, 212, 216, 0.55),
        overlay: rgba(0, 0, 0, 0.6),
        skeleton: rgb(39, 39, 42),
    },
    space: SPACE,
    radius: RADIUS,
    font: FONT,
    shadow: DARK_SHADOWS,
};

pub const ZINC_LIGHT: Tokens = Tokens {
    color: ColorTokens {
        background: rgb(255, 255, 255),
        foreground: rgb(9, 9, 11),
        card: rgb(255, 255, 255),
        card_foreground: rgb(9, 9, 11),
        popover: rgb(255, 255, 255),
        popover_foreground: rgb(9, 9, 11),
        primary: rgb(24, 24, 27),
        primary_hover: rgb(39, 39, 42),
        primary_active: rgb(52, 52, 56),
        primary_foreground: rgb(250, 250, 250),
        secondary: rgb(244, 244, 245),
        secondary_hover: rgb(228, 228, 231),
        secondary_active: rgb(212, 212, 216),
        secondary_foreground: rgb(24, 24, 27),
        muted: rgb(244, 244, 245),
        muted_foreground: rgb(113, 113, 122),
        accent: rgb(244, 244, 245),
        accent_foreground: rgb(24, 24, 27),
        destructive: rgb(239, 68, 68),
        destructive_hover: rgb(220, 38, 38),
        destructive_active: rgb(185, 28, 28),
        destructive_foreground: rgb(250, 250, 250),
        warning: rgb(202, 138, 4),
        warning_foreground: rgb(30, 20, 0),
        success: rgb(22, 163, 74),
        success_foreground: rgb(240, 253, 244),
        info: rgb(14, 165, 233),
        info_foreground: rgb(240, 249, 255),
        border: rgb(228, 228, 231),
        border_strong: rgb(212, 212, 216),
        input: rgb(228, 228, 231),
        input_foreground: rgb(9, 9, 11),
        ring: rgba(24, 24, 27, 0.45),
        overlay: rgba(10, 10, 14, 0.4),
        skeleton: rgb(228, 228, 231),
    },
    space: SPACE,
    radius: RADIUS,
    font: FONT,
    shadow: LIGHT_SHADOWS,
};

// ---------- Slate (cool blue-tinted neutral) -------------------------------

pub const SLATE_DARK: Tokens = Tokens {
    color: ColorTokens {
        background: rgb(2, 6, 23),
        foreground: rgb(248, 250, 252),
        card: rgb(2, 6, 23),
        card_foreground: rgb(248, 250, 252),
        popover: rgb(2, 6, 23),
        popover_foreground: rgb(248, 250, 252),
        primary: rgb(248, 250, 252),
        primary_hover: rgb(226, 232, 240),
        primary_active: rgb(203, 213, 225),
        primary_foreground: rgb(15, 23, 42),
        secondary: rgb(30, 41, 59),
        secondary_hover: rgb(45, 56, 74),
        secondary_active: rgb(51, 65, 85),
        secondary_foreground: rgb(248, 250, 252),
        muted: rgb(30, 41, 59),
        muted_foreground: rgb(148, 163, 184),
        accent: rgb(30, 41, 59),
        accent_foreground: rgb(248, 250, 252),
        destructive: rgb(220, 38, 38),
        destructive_hover: rgb(239, 68, 68),
        destructive_active: rgb(185, 28, 28),
        destructive_foreground: rgb(248, 250, 252),
        warning: rgb(234, 179, 8),
        warning_foreground: rgb(28, 18, 0),
        success: rgb(34, 197, 94),
        success_foreground: rgb(6, 32, 16),
        info: rgb(56, 189, 248),
        info_foreground: rgb(4, 20, 30),
        border: rgb(30, 41, 59),
        border_strong: rgb(51, 65, 85),
        input: rgb(30, 41, 59),
        input_foreground: rgb(248, 250, 252),
        ring: rgba(203, 213, 225, 0.55),
        overlay: rgba(2, 6, 23, 0.65),
        skeleton: rgb(30, 41, 59),
    },
    space: SPACE,
    radius: RADIUS,
    font: FONT,
    shadow: DARK_SHADOWS,
};

pub const SLATE_LIGHT: Tokens = Tokens {
    color: ColorTokens {
        background: rgb(255, 255, 255),
        foreground: rgb(2, 6, 23),
        card: rgb(255, 255, 255),
        card_foreground: rgb(2, 6, 23),
        popover: rgb(255, 255, 255),
        popover_foreground: rgb(2, 6, 23),
        primary: rgb(15, 23, 42),
        primary_hover: rgb(30, 41, 59),
        primary_active: rgb(51, 65, 85),
        primary_foreground: rgb(248, 250, 252),
        secondary: rgb(241, 245, 249),
        secondary_hover: rgb(226, 232, 240),
        secondary_active: rgb(203, 213, 225),
        secondary_foreground: rgb(15, 23, 42),
        muted: rgb(241, 245, 249),
        muted_foreground: rgb(100, 116, 139),
        accent: rgb(241, 245, 249),
        accent_foreground: rgb(15, 23, 42),
        destructive: rgb(239, 68, 68),
        destructive_hover: rgb(220, 38, 38),
        destructive_active: rgb(185, 28, 28),
        destructive_foreground: rgb(248, 250, 252),
        warning: rgb(202, 138, 4),
        warning_foreground: rgb(30, 20, 0),
        success: rgb(22, 163, 74),
        success_foreground: rgb(240, 253, 244),
        info: rgb(14, 165, 233),
        info_foreground: rgb(240, 249, 255),
        border: rgb(226, 232, 240),
        border_strong: rgb(203, 213, 225),
        input: rgb(226, 232, 240),
        input_foreground: rgb(2, 6, 23),
        ring: rgba(15, 23, 42, 0.45),
        overlay: rgba(10, 14, 24, 0.4),
        skeleton: rgb(226, 232, 240),
    },
    space: SPACE,
    radius: RADIUS,
    font: FONT,
    shadow: LIGHT_SHADOWS,
};

// ---------- Nanite (brand preset, blue primary) ----------------------------

pub const NANITE_DARK: Tokens = Tokens {
    color: ColorTokens {
        background: rgb(10, 10, 12),
        foreground: rgb(244, 244, 247),
        card: rgb(17, 17, 20),
        card_foreground: rgb(244, 244, 247),
        popover: rgb(20, 20, 24),
        popover_foreground: rgb(244, 244, 247),
        primary: rgb(59, 130, 246),
        primary_hover: rgb(79, 146, 255),
        primary_active: rgb(37, 109, 224),
        primary_foreground: rgb(248, 250, 252),
        secondary: rgb(32, 32, 38),
        secondary_hover: rgb(42, 42, 50),
        secondary_active: rgb(26, 26, 32),
        secondary_foreground: rgb(230, 230, 236),
        muted: rgb(28, 28, 33),
        muted_foreground: rgb(150, 150, 160),
        accent: rgb(38, 38, 46),
        accent_foreground: rgb(244, 244, 247),
        destructive: rgb(220, 38, 38),
        destructive_hover: rgb(239, 68, 68),
        destructive_active: rgb(185, 28, 28),
        destructive_foreground: rgb(254, 242, 242),
        warning: rgb(234, 179, 8),
        warning_foreground: rgb(20, 15, 0),
        success: rgb(34, 197, 94),
        success_foreground: rgb(5, 30, 15),
        info: rgb(56, 189, 248),
        info_foreground: rgb(5, 20, 28),
        border: rgb(38, 38, 45),
        border_strong: rgb(60, 60, 70),
        input: rgb(22, 22, 26),
        input_foreground: rgb(244, 244, 247),
        ring: rgba(59, 130, 246, 0.55),
        overlay: rgba(0, 0, 0, 0.55),
        skeleton: rgb(34, 34, 40),
    },
    space: SPACE,
    radius: RADIUS,
    font: FONT,
    shadow: DARK_SHADOWS,
};

pub const NANITE_LIGHT: Tokens = Tokens {
    color: ColorTokens {
        background: rgb(252, 252, 253),
        foreground: rgb(15, 18, 24),
        card: rgb(255, 255, 255),
        card_foreground: rgb(15, 18, 24),
        popover: rgb(255, 255, 255),
        popover_foreground: rgb(15, 18, 24),
        primary: rgb(37, 99, 235),
        primary_hover: rgb(29, 78, 216),
        primary_active: rgb(30, 64, 175),
        primary_foreground: rgb(248, 250, 252),
        secondary: rgb(241, 243, 247),
        secondary_hover: rgb(229, 232, 240),
        secondary_active: rgb(218, 222, 232),
        secondary_foreground: rgb(30, 36, 48),
        muted: rgb(241, 243, 247),
        muted_foreground: rgb(100, 108, 124),
        accent: rgb(237, 240, 247),
        accent_foreground: rgb(15, 18, 24),
        destructive: rgb(220, 38, 38),
        destructive_hover: rgb(185, 28, 28),
        destructive_active: rgb(153, 27, 27),
        destructive_foreground: rgb(254, 242, 242),
        warning: rgb(202, 138, 4),
        warning_foreground: rgb(28, 20, 0),
        success: rgb(22, 163, 74),
        success_foreground: rgb(240, 253, 244),
        info: rgb(14, 165, 233),
        info_foreground: rgb(240, 249, 255),
        border: rgb(225, 228, 236),
        border_strong: rgb(200, 205, 217),
        input: rgb(255, 255, 255),
        input_foreground: rgb(15, 18, 24),
        ring: rgba(37, 99, 235, 0.45),
        overlay: rgba(10, 12, 20, 0.45),
        skeleton: rgb(230, 233, 240),
    },
    space: SPACE,
    radius: RADIUS,
    font: FONT,
    shadow: LIGHT_SHADOWS,
};

// Aliases for the brand preset — primitives reference these when they need a
// default token set at construction time (e.g. padding).
pub const DARK: Tokens = NANITE_DARK;
pub const LIGHT: Tokens = NANITE_LIGHT;

// ---------- preset + mode switching ----------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Zinc,
    Slate,
    Nanite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

impl Preset {
    pub const ALL: &'static [Preset] = &[Preset::Zinc, Preset::Slate, Preset::Nanite];

    pub fn display_name(self) -> &'static str {
        match self {
            Preset::Zinc => "Zinc",
            Preset::Slate => "Slate",
            Preset::Nanite => "Nanite",
        }
    }

    pub fn tokens(self, mode: Mode) -> &'static Tokens {
        match (self, mode) {
            (Preset::Zinc, Mode::Dark) => &ZINC_DARK,
            (Preset::Zinc, Mode::Light) => &ZINC_LIGHT,
            (Preset::Slate, Mode::Dark) => &SLATE_DARK,
            (Preset::Slate, Mode::Light) => &SLATE_LIGHT,
            (Preset::Nanite, Mode::Dark) => &NANITE_DARK,
            (Preset::Nanite, Mode::Light) => &NANITE_LIGHT,
        }
    }

    pub fn theme_name(self, mode: Mode) -> &'static str {
        match (self, mode) {
            (Preset::Zinc, Mode::Dark) => "Nanite Zinc Dark",
            (Preset::Zinc, Mode::Light) => "Nanite Zinc Light",
            (Preset::Slate, Mode::Dark) => "Nanite Slate Dark",
            (Preset::Slate, Mode::Light) => "Nanite Slate Light",
            (Preset::Nanite, Mode::Dark) => "Nanite Dark",
            (Preset::Nanite, Mode::Light) => "Nanite Light",
        }
    }

    /// Build an `iced::Theme::Custom` named after this preset+mode. The
    /// returned theme can be handed directly to `iced::daemon().theme(...)`.
    pub fn iced_theme(self, mode: Mode) -> Theme {
        let tokens = self.tokens(mode);
        let palette = palette_from_tokens(tokens);
        Theme::custom_with_fn(self.theme_name(mode), palette, move |p| {
            extended_from_tokens(p, tokens, mode)
        })
    }
}

fn palette_from_tokens(tokens: &Tokens) -> Palette {
    let c = &tokens.color;
    Palette {
        background: c.background,
        text: c.foreground,
        primary: c.primary,
        success: c.success,
        warning: c.warning,
        danger: c.destructive,
    }
}

fn extended_from_tokens(p: Palette, _tokens: &Tokens, _mode: Mode) -> palette::Extended {
    // Use iced's own extended palette generator as a sensible default; our
    // primitives mostly bypass it by reading `Tokens` directly via
    // `tokens_for`, but any built-in iced widget that isn't wrapped yet
    // still picks up a coherent look from the base palette.
    palette::Extended::generate(p)
}

/// Resolve the token set for any `iced::Theme` passed in to a style closure.
///
/// Matches on the custom theme name produced by [`Preset::iced_theme`]; falls
/// back to Nanite Dark / Light (by iced's own light/dark detection) for any
/// unknown built-in theme.
pub fn tokens_for(theme: &Theme) -> &'static Tokens {
    match theme.name() {
        "Nanite Zinc Dark" => &ZINC_DARK,
        "Nanite Zinc Light" => &ZINC_LIGHT,
        "Nanite Slate Dark" => &SLATE_DARK,
        "Nanite Slate Light" => &SLATE_LIGHT,
        "Nanite Dark" => &NANITE_DARK,
        "Nanite Light" => &NANITE_LIGHT,
        _ => match theme {
            Theme::Light
            | Theme::GruvboxLight
            | Theme::SolarizedLight
            | Theme::TokyoNightLight
            | Theme::CatppuccinLatte => &NANITE_LIGHT,
            _ => &NANITE_DARK,
        },
    }
}

/// Helper to build an `iced::border::Border` from a token radius.
pub fn border(color: Color, width: f32, radius: f32) -> border::Border {
    border::Border {
        color,
        width,
        radius: radius.into(),
    }
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

pub fn with_alpha(color: Color, factor: f32) -> Color {
    Color {
        a: (color.a * factor).clamp(0.0, 1.0),
        ..color
    }
}
