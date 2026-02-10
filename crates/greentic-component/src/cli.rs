use anyhow::{Error, Result, bail};
use clap::{Parser, Subcommand};

use crate::cmd::{
    self, build::BuildArgs, doctor::DoctorArgs, flow::FlowCommand, hash::HashArgs,
    inspect::InspectArgs, new::NewArgs, store::StoreCommand, templates::TemplatesArgs,
    test::TestArgs, wizard::WizardCommand,
};
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
    /// Component wizard helpers
    #[command(subcommand)]
    Wizard(WizardCommand),
    /// List available component templates
    Templates(TemplatesArgs),
    /// Run component doctor checks
    Doctor(DoctorArgs),
    /// Inspect manifests and describe payloads
    Inspect(InspectArgs),
    /// Recompute manifest hashes
    Hash(HashArgs),
    /// Build component wasm + update config flows
    Build(BuildArgs),
    /// Invoke a component locally with an in-memory state/secrets harness
    #[command(
        long_about = "Invoke a component locally with in-memory state/secrets. \
See docs/component-developer-guide.md for a walkthrough."
    )]
    Test(Box<TestArgs>),
    /// Flow utilities (config flow regeneration)
    #[command(subcommand)]
    Flow(FlowCommand),
    /// Interact with the component store
    #[command(subcommand)]
    Store(StoreCommand),
}

pub fn main() -> Result<()> {
    let cli = Cli::parse();
    let engine = ScaffoldEngine::new();
    match cli.command {
        Commands::New(args) => cmd::new::run(args, &engine),
        Commands::Wizard(command) => cmd::wizard::run(command),
        Commands::Templates(args) => cmd::templates::run(args, &engine),
        Commands::Doctor(args) => cmd::doctor::run(args).map_err(Error::new),
        Commands::Inspect(args) => {
            let result = cmd::inspect::run(&args)?;
            cmd::inspect::emit_warnings(&result.warnings);
            if args.strict && !result.warnings.is_empty() {
                bail!(
                    "component-inspect: {} warning(s) treated as errors (--strict)",
                    result.warnings.len()
                );
            }
            Ok(())
        }
        Commands::Hash(args) => cmd::hash::run(args),
        Commands::Build(args) => cmd::build::run(args),
        Commands::Test(args) => cmd::test::run(*args),
        Commands::Flow(flow_cmd) => cmd::flow::run(flow_cmd),
        Commands::Store(store_cmd) => cmd::store::run(store_cmd),
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
                assert!(!args.no_git);
            }
            _ => panic!("expected new args"),
        }
    }

    #[test]
    fn parses_wizard_new_subcommand() {
        let cli = Cli::try_parse_from([
            "greentic-component",
            "wizard",
            "new",
            "demo-component",
            "--abi-version",
            "0.6.0",
        ])
        .expect("expected CLI to parse");
        match cli.command {
            Commands::Wizard(command) => match command {
                WizardCommand::New(args) => {
                    assert_eq!(args.name, "demo-component");
                    assert_eq!(args.abi_version, "0.6.0");
                }
            },
            _ => panic!("expected wizard args"),
        }
    }
}
