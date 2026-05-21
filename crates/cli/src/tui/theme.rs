use ratatui::style::Color;

#[derive(Debug, Clone)]
pub struct Palette {
    pub bg: Color,
    pub surface: Color,
    pub text: Color,
    pub text_dim: Color,
    pub accent: Color,
    pub user_bubble: Color,
    pub assistant_bubble: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub code_bg: Color,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub palette: Palette,
    pub name: String,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            name: "dark".into(),
            palette: Palette {
                bg: Color::Rgb(18, 18, 24),
                surface: Color::Rgb(28, 28, 36),
                text: Color::Rgb(220, 220, 230),
                text_dim: Color::Rgb(120, 120, 140),
                accent: Color::Rgb(99, 150, 240),
                user_bubble: Color::Rgb(60, 100, 200),
                assistant_bubble: Color::Rgb(40, 44, 52),
                success: Color::Rgb(80, 200, 120),
                warning: Color::Rgb(220, 180, 60),
                error: Color::Rgb(220, 80, 80),
                code_bg: Color::Rgb(22, 22, 30),
            },
        }
    }

    pub fn light() -> Self {
        Self {
            name: "light".into(),
            palette: Palette {
                bg: Color::Rgb(248, 248, 252),
                surface: Color::Rgb(236, 236, 244),
                text: Color::Rgb(24, 24, 32),
                text_dim: Color::Rgb(140, 140, 155),
                accent: Color::Rgb(40, 80, 200),
                user_bubble: Color::Rgb(60, 110, 220),
                assistant_bubble: Color::Rgb(228, 232, 240),
                success: Color::Rgb(30, 160, 80),
                warning: Color::Rgb(180, 140, 30),
                error: Color::Rgb(200, 50, 50),
                code_bg: Color::Rgb(236, 236, 244),
            },
        }
    }
}
