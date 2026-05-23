use ratatui::style::Color;

#[derive(Debug, Clone)]
#[allow(dead_code)]
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
    #[allow(dead_code)]
    pub name: String,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            name: "dark".into(),
            palette: Palette {
                bg: Color::Rgb(13, 13, 13),
                surface: Color::Rgb(24, 24, 24),
                text: Color::Rgb(204, 204, 204),
                text_dim: Color::Rgb(102, 102, 102),
                accent: Color::Rgb(110, 160, 255),
                user_bubble: Color::Rgb(180, 180, 180),
                assistant_bubble: Color::Rgb(36, 36, 36),
                success: Color::Rgb(72, 199, 142),
                warning: Color::Rgb(214, 174, 60),
                error: Color::Rgb(235, 87, 87),
                code_bg: Color::Rgb(20, 20, 20),
            },
        }
    }

    #[allow(dead_code)]
    pub fn light() -> Self {
        Self {
            name: "light".into(),
            palette: Palette {
                bg: Color::Rgb(255, 255, 255),
                surface: Color::Rgb(242, 242, 242),
                text: Color::Rgb(28, 28, 28),
                text_dim: Color::Rgb(128, 128, 128),
                accent: Color::Rgb(50, 100, 220),
                user_bubble: Color::Rgb(60, 60, 60),
                assistant_bubble: Color::Rgb(245, 245, 245),
                success: Color::Rgb(30, 160, 80),
                warning: Color::Rgb(180, 140, 30),
                error: Color::Rgb(200, 50, 50),
                code_bg: Color::Rgb(246, 246, 246),
            },
        }
    }
}
