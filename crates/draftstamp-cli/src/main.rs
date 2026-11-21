//! The `draftstamp` command.

use std::path::PathBuf;
use std::process::ExitCode;

use draftstamp_cli::{Context, http::Native, run};

fn main() -> ExitCode {
    let Ok(cwd) = std::env::current_dir() else {
        eprintln!("Cannot read the current directory.");
        return ExitCode::from(1);
    };
    let default_vault = std::env::var_os("DRAFTSTAMP_VAULT")
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".dsvault")))
        .unwrap_or_else(|| cwd.join(".dsvault"));
    let mut ctx = Context {
        cwd,
        default_vault,
        out: &mut std::io::stdout(),
        err: &mut std::io::stderr(),
    };
    ExitCode::from(run(std::env::args_os(), &Native::new(), &mut ctx))
}
