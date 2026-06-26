mod cli;
mod commands;
mod config;
mod git;
mod snapshot;
mod source;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    // Intercept Ctrl-C so cliclack prompts cancel gracefully (like ESC)
    // instead of killing the process with the cursor hidden.
    ctrlc::set_handler(|| {})
        .map_err(|error| anyhow::anyhow!("setting Ctrl-C handler: {error}"))?;

    let cli = cli::Cli::parse();
    commands::run(cli)
}
