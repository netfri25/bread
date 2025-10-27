use clap::Parser;

use crate::pixels::Color;

#[derive(Parser)]
pub struct Config {
    #[arg(long, short)]
    pub top: bool,

    #[arg(short, long)]
    pub font: Option<String>,

    #[arg(long, short = 's', default_value_t = 24)]
    pub font_size: u32,

    #[arg(long, value_parser = parse_color, default_value = "ffffff")]
    pub fg: Color,

    #[arg(long, value_parser = parse_color, default_value = "000000")]
    pub bg: Color,

    #[arg(long, default_value_t = 24)]
    pub height: u32,
}

fn parse_color(s: &str) -> Result<Color, &'static str> {
    s.parse().map_err(|_| "invalid color")
}
