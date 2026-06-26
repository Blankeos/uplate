use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "uplate")]
#[command(version, about = "Clone and upgrade detached boilerplates")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Template source for shorthand create, e.g. `uplate source destination`
    pub source: Option<String>,

    /// Destination directory for shorthand create
    pub destination: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a new project from a boilerplate
    Create(CreateArgs),
    /// Adopt an existing project created from a boilerplate
    Adopt(AdoptArgs),
    /// Show whether a boilerplate update is available
    Status(StatusArgs),
    /// Upgrade the current project from its boilerplate source
    Upgrade(UpgradeArgs),
}

#[derive(Debug, Parser)]
pub struct CreateArgs {
    /// Template source, e.g. `owner/repo/path` or `<https://gitlab.com/group/project>`
    pub source: Option<String>,
    /// Destination directory. Defaults to current directory in interactive mode.
    pub destination: Option<PathBuf>,
}

#[derive(Debug, Parser)]
pub struct AdoptArgs {
    /// Template source, e.g. `owner/repo/path` or `<https://gitlab.com/group/project>`
    pub source: Option<String>,
    /// Base commit or tag of the boilerplate that this project was created from
    #[arg(long)]
    pub base: Option<String>,
}

#[derive(Debug, Parser)]
pub struct StatusArgs {}

#[derive(Debug, Parser)]
pub struct UpgradeArgs {
    /// Preview the upgrade without modifying the working tree
    #[arg(long)]
    pub dry_run: bool,
    /// Print a conflict-resolution prompt for coding agents if the upgrade cannot be applied cleanly
    #[arg(long)]
    pub prompt: bool,
}
