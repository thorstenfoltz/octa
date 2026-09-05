//! `--completions <SHELL>`: print a shell completion script to stdout.
//!
//! Generated from the same `#[derive(Parser)]` surface the CLI already parses
//! with, so every flag documented in `--help` is completed without a second
//! list to keep in step. Printing to stdout rather than writing files is what
//! makes it composable: `eval "$(octa --completions zsh)"` in a shell rc, or
//! `install.sh` redirecting it into the per-shell directories.

use clap::CommandFactory;
use clap_complete::Shell;

use super::args::Cli;

pub fn run(shell: Shell) -> anyhow::Result<()> {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "octa", &mut std::io::stdout().lock());
    Ok(())
}
