use ab_glyph::{Font, PxScaleFont, ScaleFont as _, point};

use crate::parser::Size;
use crate::pixels::{Color, Pixels};

pub struct DrawState<'pixels, 'font, F: Font> {
    pixels: &'pixels mut Pixels,
    font: &'font PxScaleFont<F>,
    x: f32,
    fg: Color,
    bg: Color,
}

impl<'pixels, 'font, F: Font> DrawState<'pixels, 'font, F> {
    pub fn new(
        pixels: &'pixels mut Pixels,
        font: &'font PxScaleFont<F>,
        start_x: f32,
        fg: Color,
        bg: Color,
    ) -> Self {
        let x = start_x;

        Self {
            pixels,
            font,
            x,
            fg,
            bg,
        }
    }

    pub fn set_fg(&mut self, fg: Color) {
        self.fg = fg;
    }

    pub fn set_bg(&mut self, bg: Color) {
        self.bg = bg;
    }

    pub fn draw_text(&mut self, text: &str) {
        let center_y = (self.pixels.height() as f32 - self.font.height()) / 2.;
        let center_y = center_y as i32;

        for c in text.chars() {
            let mut glyph = self.font.scaled_glyph(c);
            glyph.position = point(self.x, 0.);

            let h_advance = self.font.h_advance(glyph.id);

            // fill background
            for off_x in 0..h_advance.ceil() as u32 {
                for y in 0..self.pixels.height() {
                    let x = self.x as u32 + off_x;
                    self.pixels.set(x, y, self.bg);
                }
            }

            let Some(outline) = self.font.outline_glyph(glyph) else {
                self.x += h_advance;
                continue;
            };

            let bounds = outline.px_bounds();

            outline.draw(|x, y, f| {
                let x = bounds.min.x as i32 + x as i32;
                let y = center_y + bounds.min.y as i32 + y as i32 + self.font.ascent() as i32;
                if x < 0 || y < 0 {
                    return;
                }

                let x = x as u32;
                let y = y as u32;

                let f = f.clamp(0., 1.);
                let color = self.bg.interpolate(self.fg, f);

                self.pixels.set(x, y, color);
            });

            self.x += h_advance;
        }
    }

    pub fn draw_ramp(&mut self, size: Size) {
        let max_y = self.pixels.height() - size.h;
        for x in 0..size.w {
            let x = self.x.ceil() as u32 + x;
            for y in 0..=max_y {
                self.pixels.set(x, y, self.bg);
            }

            for y in max_y + 1..self.pixels.height() {
                self.pixels.set(x, y, self.fg);
            }
        }

        self.x += size.w as f32;
    }
}
