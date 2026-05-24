mod cli;
mod error;
mod input;
mod parquet_cli;
mod render;

use std::ffi::OsString;
use std::io::Write;
use std::process::ExitCode;

use cli::{Action, Command};
use error::Result;

pub use cli::OutputFormat;

pub fn main_entry<I, W, E>(args: I, stdout: W, mut stderr: E) -> ExitCode
where
    I: IntoIterator<Item = OsString>,
    W: Write,
    E: Write,
{
    main_entry_with_default_format(args, stdout, &mut stderr, OutputFormat::Jsonl)
}

pub fn main_entry_with_default_format<I, W, E>(
    args: I,
    stdout: W,
    mut stderr: E,
    default_format: OutputFormat,
) -> ExitCode
where
    I: IntoIterator<Item = OsString>,
    W: Write,
    E: Write,
{
    match run(args, stdout, &mut stderr, default_format) {
        Ok(code) => code,
        Err(err) if err.is_broken_pipe() => ExitCode::SUCCESS,
        Err(err) => {
            let _ = writeln!(stderr, "error: {err}");
            ExitCode::from(1)
        }
    }
}

fn run<I, W, E>(
    args: I,
    mut stdout: W,
    mut stderr: E,
    default_format: OutputFormat,
) -> Result<ExitCode>
where
    I: IntoIterator<Item = OsString>,
    W: Write,
    E: Write,
{
    match cli::parse(args) {
        Ok(Action::Help(help)) => {
            stdout.write_all(help.as_bytes())?;
            Ok(ExitCode::SUCCESS)
        }
        Ok(Action::Version(version)) => {
            writeln!(stdout, "{version}")?;
            Ok(ExitCode::SUCCESS)
        }
        Ok(Action::Run(command)) => {
            run_command(command, default_format, &mut stdout)?;
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            writeln!(stderr, "error: {}", err.message)?;
            writeln!(stderr)?;
            stderr.write_all(err.help.as_bytes())?;
            Ok(ExitCode::from(2))
        }
    }
}

fn run_command(
    command: Command,
    default_format: OutputFormat,
    stdout: &mut impl Write,
) -> Result<()> {
    let source = input::open(command.file())?;
    parquet_cli::run(
        source.file,
        command.mode(),
        command.output_format(default_format),
        stdout,
    )
}
