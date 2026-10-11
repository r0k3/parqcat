use std::io::{BufWriter, IsTerminal};
use std::process::ExitCode;

fn main() -> ExitCode {
    let stdout = std::io::stdout();
    let default_format = if stdout.is_terminal() {
        parqcat::OutputFormat::Table
    } else {
        parqcat::OutputFormat::Jsonl
    };

    parqcat::main_entry_with_default_format(
        std::env::args_os().skip(1),
        BufWriter::new(stdout.lock()),
        std::io::stderr(),
        default_format,
    )
}
