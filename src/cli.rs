use clap::{Parser, Subcommand};
#[derive(Parser)]
#[command(
    name = "codex2api",
    version,
    about = "Expose a separate ChatGPT Codex account through a local Responses API"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}
#[derive(Subcommand)]
pub enum Command {
    Init,
    Login {
        #[arg(long)]
        no_open: bool,
    },
    Serve {
        #[arg(long)]
        allow_non_loopback: bool,
    },
    Status,
    Logout,
    Key {
        #[command(subcommand)]
        command: KeyCommand,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}
#[derive(Subcommand)]
pub enum KeyCommand {
    Rotate,
    Show,
}
#[derive(Subcommand)]
pub enum ConfigCommand {
    Path,
}
