use std::path::PathBuf;

use clap::Parser;

use crate::pixels::Color;

#[derive(Parser)]
pub struct Config {
    #[arg(short, long, value_parser = get_font)]
    pub font: PathBuf,

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

fn get_font(s: &str) -> Result<PathBuf, String> {
    findfont::find(s).ok_or_else(|| format!("unable to find font: {}", s))
}
