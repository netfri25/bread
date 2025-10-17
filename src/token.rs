use ab_glyph::{Font, PxScaleFont, ScaleFont as _};

use crate::parser::{Alignment, Size};
use crate::pixels::Color;

#[derive(Debug, Clone, Copy)]
pub enum Token<'a> {
    /// simple text
    Text(&'a str),

    /// change of alignment
    /// %{l} %{c} %{r}
    Alignment(Alignment),

    /// change of foreground color
    /// %{F:[AA]RRGGBB}
    Fg(Color),

    /// change of background color
    /// %{B:[AA]RRGGBB}
    Bg(Color),

    /// underline
    /// %{U:WxH}
    Underline(Size),

    /// overline
    /// %{O:WxH}
    Overline(Size),
}

impl Token<'_> {
    pub fn px_width<F: Font>(&self, font: impl AsRef<PxScaleFont<F>>) -> u32 {
        let font = font.as_ref();

        match self {
            Token::Text(text) => text
                .chars()
                .map(|c| font.h_advance(font.glyph_id(c)))
                .sum::<f32>() as u32,

            Token::Underline(size) => size.w,
            Token::Overline(size) => size.w,
            _ => 0,
        }
    }
}
