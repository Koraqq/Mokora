use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "mokora", about = "Mokora - Mahjong AI Assistant")]
pub struct Cli {
    #[arg(short, long, help = "Path to config.toml")]
    pub config: Option<PathBuf>,
}
