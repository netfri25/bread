use ab_glyph::{Font, PxScaleFont, ScaleFont as _};

use crate::parser::{Section, Size};
use crate::pixels::Color;

#[derive(Debug, Clone, Copy)]
pub enum Token<'a> {
    /// simple text
    Text(&'a str),

    /// change of section
    /// %{l} %{c} %{r}
    Section(Section),

    /// change of foreground color
    /// %{F:[AA]RRGGBB}
    Fg(Color),

    /// change of background color
    /// %{B:[AA]RRGGBB}
    Bg(Color),

    /// ramp
    /// %{R:WxH}
    Ramp(Size),
}

impl Token<'_> {
    pub fn px_width<F: Font>(&self, font: &PxScaleFont<F>) -> f32 {
        match self {
            Token::Text(text) => text.chars().map(|c| font.h_advance(font.glyph_id(c))).sum(),
            Token::Ramp(size) => size.w as f32,
            _ => 0.,
        }
    }
}
