#![cfg(feature = "cli")]

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::cmd::{self, new::NewArgs, templates::TemplatesArgs};
use crate::scaffold::engine::ScaffoldEngine;

#[derive(Parser, Debug)]
#[command(
    name = "greentic-component",
    about = "Toolkit for Greentic component developers",
    version,
    propagate_version = true,
    arg_required_else_help = true,
    disable_version_flag = true
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Scaffold a new Greentic component project
    New(NewArgs),
    /// List available component templates
    Templates(TemplatesArgs),
}

pub fn main() -> Result<()> {
    let cli = Cli::parse();
    let engine = ScaffoldEngine::new();
    match cli.command {
        Commands::New(args) => cmd::new::run(args, &engine),
        Commands::Templates(args) => cmd::templates::run(args, &engine),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_new_subcommand() {
        let cli = Cli::try_parse_from(["greentic-component", "new", "--name", "demo", "--json"])
            .expect("expected CLI to parse");
        match cli.command {
            Commands::New(args) => {
                assert_eq!(args.name, "demo");
                assert!(args.json);
                assert!(!args.no_check);
            }
            Commands::Templates(_) => panic!("expected new args"),
        }
    }
}
