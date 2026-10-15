//! Colours and the few container and button styles the shell uses. One accent
//! colour; a calm dark palette by default, light when the OS asks for it.

use iced::border::Radius;
use iced::widget::{button, container};
use iced::{Background, Border, Color, Shadow, Theme, Vector, theme::Palette};

/// Every colour the views use. `Copy` so closures can capture it by value.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub light: bool,
    pub bg: Color,
    pub panel: Color,
    pub card: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub ok: Color,
    pub warn: Color,
    pub error: Color,
}

fn hex(v: u32) -> Color {
    Color::from_rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

impl Colors {
    pub fn dark() -> Self {
        Colors {
            light: false,
            bg: hex(0x14161b),
            panel: hex(0x1a1d24),
            card: hex(0x20242d),
            border: hex(0x2c313c),
            text: hex(0xe4e7ec),
            muted: hex(0x8b93a1),
            accent: hex(0x7aa2f7),
            on_accent: hex(0x0d1117),
            ok: hex(0x9ece6a),
            warn: hex(0xe0af68),
            error: hex(0xf7768e),
        }
    }

    pub fn light() -> Self {
        Colors {
            light: true,
            bg: hex(0xf6f7f9),
            panel: hex(0xeceef2),
            card: hex(0xffffff),
            border: hex(0xd5d9e0),
            text: hex(0x1d2129),
            muted: hex(0x6b7280),
            accent: hex(0x3b6fd8),
            on_accent: hex(0xffffff),
            ok: hex(0x2f855a),
            warn: hex(0xb7791f),
            error: hex(0xc0392b),
        }
    }

    /// The iced theme built from the same colours, so built-in widgets match.
    pub fn theme(&self) -> Theme {
        Theme::custom(
            if self.light { "Locus Light" } else { "Locus Dark" }.to_string(),
            Palette {
                background: self.bg,
                text: self.text,
                primary: self.accent,
                success: self.ok,
                warning: self.warn,
                danger: self.error,
            },
        )
    }

    /// Fill for a selected row or a selected calendar day.
    pub fn selected_fill(&self) -> Color {
        with_alpha(self.accent, 0.18)
    }
}

/// A rounded card with a 1px border.
pub fn card(c: Colors) -> container::Style {
    container::Style {
        text_color: Some(c.text),
        background: Some(Background::Color(c.card)),
        border: Border {
            color: c.border,
            width: 1.0,
            radius: Radius::from(6.0),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// The sidebar and the status bar.
pub fn panel(c: Colors) -> container::Style {
    container::Style {
        text_color: Some(c.text),
        background: Some(Background::Color(c.panel)),
        border: Border {
            color: c.border,
            width: 0.0,
            radius: Radius::from(0.0),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

/// Dim layer behind an overlay.
pub fn scrim(c: Colors) -> container::Style {
    let _ = c;
    container::Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.35))),
        ..container::Style::default()
    }
}

/// Flat button for list rows and nav items. `selected` fills with the accent
/// tint; hover lightens the background slightly.
pub fn row_button(
    c: Colors,
    selected: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
    move |_, status| {
        let bg = if selected {
            c.selected_fill()
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            with_alpha(c.text, 0.06)
        } else {
            Color::TRANSPARENT
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: c.text,
            border: Border {
                color: if selected { with_alpha(c.accent, 0.5) } else { Color::TRANSPARENT },
                width: 1.0,
                radius: Radius::from(6.0),
            },
            shadow: Shadow::default(),
            snap: false,
        }
    }
}

/// A filled accent button (primary actions in dialogs).
pub fn primary_button(c: Colors) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
    move |_, status| {
        let bg = if matches!(status, button::Status::Hovered) {
            with_alpha(c.accent, 0.85)
        } else {
            c.accent
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: c.on_accent,
            border: Border {
                radius: Radius::from(6.0),
                ..Border::default()
            },
            shadow: Shadow {
                color: Color::TRANSPARENT,
                offset: Vector::ZERO,
                blur_radius: 0.0,
            },
            snap: false,
        }
    }
}

/// An outlined button (toolbar actions, secondary actions in dialogs).
/// Disabled, its text is dimmed.
pub fn quiet_button(c: Colors) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
    move |_, status| {
        let bg = if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            with_alpha(c.text, 0.06)
        } else {
            Color::TRANSPARENT
        };
        let text_color = if matches!(status, button::Status::Disabled) {
            with_alpha(c.text, 0.35)
        } else {
            c.text
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color,
            border: Border {
                color: c.border,
                width: 1.0,
                radius: Radius::from(6.0),
            },
            shadow: Shadow::default(),
            snap: false,
        }
    }
}

/// A large card-like button: the app tiles on the launcher page.
pub fn tile_button(c: Colors) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(if hovered { c.selected_fill() } else { c.card })),
            text_color: c.text,
            border: Border {
                color: if hovered { with_alpha(c.accent, 0.5) } else { c.border },
                width: 1.0,
                radius: Radius::from(10.0),
            },
            shadow: Shadow::default(),
            snap: false,
        }
    }
}

/// Background of a list row drawn without a button (rows that tell a
/// single click from a double click).
pub fn row_fill(c: Colors, selected: bool) -> container::Style {
    container::Style {
        text_color: Some(c.text),
        background: selected.then(|| Background::Color(c.selected_fill())),
        border: Border {
            color: if selected { with_alpha(c.accent, 0.5) } else { Color::TRANSPARENT },
            width: 1.0,
            radius: Radius::from(6.0),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}
