use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Arguments {
    /// Path to power systems data pack. This should in the
    /// `PowerSystemsData` format
    pub pack_path: PathBuf,

    /// Set the port of the server
    #[arg(short, long)]
    pub port: Option<u16>,
}
