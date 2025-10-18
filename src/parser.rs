use std::iter;

use crate::pixels::Color;
use crate::token::Token;

pub fn parse<'a>(mut input: &'a str) -> impl Iterator<Item = Token<'a>> {
    iter::from_fn(move || {
        if input.is_empty() {
            return None;
        }

        let token;
        (token, input) = parse_token(input);
        Some(token)
    })
}

#[derive(Debug, Clone, Copy)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum Section {
    Left,
    Center,
    Right,
}

fn parse_token<'a>(input: &'a str) -> (Token<'a>, &'a str) {
    if let Some((token, input)) = parse_non_text(input) {
        return (token, input);
    }

    if let Some(mut index) = input.find('%') {
        if index == 0 {
            index = 1 + input[1..]
                .find('%')
                .unwrap_or(input.len().saturating_sub(1));
        }

        let (text, input) = input.split_at(index);
        let token = Token::Text(text);
        (token, input)
    } else {
        let token = Token::Text(input);
        (token, "")
    }
}

fn parse_non_text<'a>(mut input: &'a str) -> Option<(Token<'a>, &'a str)> {
    input = input.strip_prefix("%{")?;
    let mut chars = input.chars();
    let c = chars.next()?;
    input = chars.as_str();

    let token = match c {
        'l' => Token::Section(Section::Left),
        'c' => Token::Section(Section::Center),
        'r' => Token::Section(Section::Right),

        'F' => {
            input = input.strip_prefix(":")?;
            let color;
            (color, input) = parse_color(input)?;
            Token::Fg(color)
        }

        'B' => {
            input = input.strip_prefix(":")?;
            let color;
            (color, input) = parse_color(input)?;
            Token::Bg(color)
        }

        'R' => {
            input = input.strip_prefix(":")?;
            let size;
            (size, input) = parse_size(input)?;
            Token::Ramp(size)
        }

        _ => return None,
    };

    input = input.strip_prefix('}')?;

    Some((token, input))
}

fn parse_size(mut input: &str) -> Option<(Size, &str)> {
    let len = input.find('}')?;
    let content;
    (content, input) = input.split_at(len);

    let (w, h) = content.split_once('x')?;

    let w = w.parse().ok()?;
    let h = h.parse().ok()?;
    let size = Size { w, h };

    Some((size, input))
}

fn parse_color(mut input: &str) -> Option<(Color, &str)> {
    let index = input.find('}')?;

    let color_text;
    (color_text, input) = input.split_at(index);

    let value = u32::from_str_radix(color_text, 16).ok()?;
    let [a, r, g, b] = value.to_be_bytes();
    let mut color = Color::new(r, g, b, a);

    match color_text.len() {
        6 => color.a = 0xFF,
        8 => {}
        _ => return None,
    }

    Some((color, input))
}
