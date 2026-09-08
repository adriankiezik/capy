use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum, error::ErrorKind};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "cargo style",
    version,
    about = "Check or fix repository source style",
    after_help = "Without a subcommand, checks crates/. Consecutive assignments stay grouped.\nCargo target metadata identifies self-crate paths. Ambiguous ownership, shadowing,\nand macro token bodies are preserved rather than guessed."
)]
pub struct Cli {
    #[command(subcommand)]
    pub action: Option<Action>,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    #[command(about = "Report violations without changing files")]
    Check(CheckInput),
    #[command(about = "Fix spacing, comments, self-crate paths and formatting")]
    Fix(Input),
}

#[derive(Debug, Args)]
pub struct CheckInput {
    #[command(flatten)]
    pub input: Input,

    #[arg(long, hide = true, value_name = "FILE", conflicts_with_all = ["paths", "stdin", "crate_name"])]
    pub staged_changes: Option<PathBuf>,
}

#[derive(Debug, Default, Args)]
pub struct Input {
    #[arg(
        value_name = "PATH",
        conflicts_with = "stdin",
        help = "Files or directories; defaults to crates/"
    )]
    pub paths: Vec<PathBuf>,

    #[arg(
        long,
        value_enum,
        value_name = "LANGUAGE",
        help = "Read source from standard input"
    )]
    pub stdin: Option<Language>,

    #[arg(long, requires = "stdin", value_name = "NAME", value_parser = crate_name, help = "Self-crate name for Rust stdin; hyphens become underscores")]
    pub crate_name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Language {
    Rs,
    Wgsl,
}

impl Cli {
    pub fn read() -> Self {
        let cli = Self::parse();

        let input = match &cli.action {
            Some(Action::Check(check)) => Some(&check.input),
            Some(Action::Fix(input)) => Some(input),
            None => None,
        };

        if let Some(input) = input
            && input.crate_name.is_some()
            && input.stdin != Some(Language::Rs)
        {
            Self::command()
                .error(
                    ErrorKind::ArgumentConflict,
                    "--crate-name requires --stdin rs",
                )
                .exit();
        }

        cli
    }
}

fn crate_name(value: &str) -> Result<String, String> {
    let name = value.replace('-', "_");

    syn::parse_str::<syn::Ident>(&name)
        .map_err(|error| format!("invalid Rust crate name: {error}"))?;

    Ok(name)
}
