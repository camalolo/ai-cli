use ratatui::{
    style::{Color, Style},
    widgets::{Block, Borders},
};

pub(crate) struct Theme {
    pub(crate) background: Color,         // #0a0a0a
    pub(crate) background_panel: Color,   // #141414
    pub(crate) background_element: Color, // #1e1e1e
    pub(crate) text: Color,               // #eeeeee
    pub(crate) text_muted: Color,         // #808080
    pub(crate) primary: Color,            // #fab283 (orange)
    pub(crate) accent: Color,             // #9d7cd8 (purple)
    pub(crate) error: Color,              // #e06c75
    pub(crate) success: Color,            // #7fd88f
    pub(crate) warning: Color,            // #f5a742
    pub(crate) info: Color,               // #56b6c2
    pub(crate) border: Color,             // #484848
    pub(crate) border_active: Color,      // #606060
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color::Rgb(10, 10, 10),
            background_panel: Color::Rgb(20, 20, 20),
            background_element: Color::Rgb(30, 30, 30),
            text: Color::Rgb(238, 238, 238),
            text_muted: Color::Rgb(128, 128, 128),
            primary: Color::Rgb(250, 178, 131),
            accent: Color::Rgb(157, 124, 216),
            error: Color::Rgb(224, 108, 117),
            success: Color::Rgb(127, 216, 143),
            warning: Color::Rgb(245, 167, 66),
            info: Color::Rgb(86, 182, 194),
            border: Color::Rgb(72, 72, 72),
            border_active: Color::Rgb(96, 96, 96),
        }
    }
}

impl Theme {
    pub(crate) fn text_style(&self) -> Style {
        Style::default().fg(self.text)
    }

    pub(crate) fn muted_style(&self) -> Style {
        Style::default().fg(self.text_muted)
    }

    pub(crate) fn primary_style(&self) -> Style {
        Style::default().fg(self.primary)
    }

    pub(crate) fn accent_style(&self) -> Style {
        Style::default().fg(self.accent)
    }

    pub(crate) fn error_style(&self) -> Style {
        Style::default().fg(self.error)
    }

    pub(crate) fn success_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    pub(crate) fn warning_style(&self) -> Style {
        Style::default().fg(self.warning)
    }

    pub(crate) fn info_style(&self) -> Style {
        Style::default().fg(self.info)
    }

    pub(crate) fn panel_block<'a>(&self, title: Option<&'a str>) -> Block<'a> {
        let mut block = Block::default()
            .borders(Borders::NONE)
            .style(Style::default().bg(self.background_panel));

        if let Some(title_str) = title {
            block = block.title(title_str).title_style(self.muted_style());
        }

        block
    }
}

pub(crate) const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(crate) fn spinner_frame(tick: u32) -> &'static str {
    SPINNER_FRAMES[(tick as usize) % SPINNER_FRAMES.len()]
}
