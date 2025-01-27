use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Arguments {
    pub pack_path: PathBuf,

    #[arg(short, long)]
    pub port: Option<u16>,
}
