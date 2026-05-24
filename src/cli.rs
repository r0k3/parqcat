use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Cat,
    Head { lines: usize },
    Tail { lines: usize },
    Schema,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Jsonl,
    Table,
}

#[derive(Debug)]
pub struct Command {
    mode: Mode,
    output_format: Option<OutputFormat>,
    file: PathBuf,
}

impl Command {
    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    pub fn output_format(&self, default_format: OutputFormat) -> OutputFormat {
        self.output_format.unwrap_or(default_format)
    }
}

#[derive(Debug)]
pub enum Action {
    Help(String),
    Version(String),
    Run(Command),
}

#[derive(Debug)]
pub struct UsageError {
    pub message: String,
    pub help: String,
}

pub fn parse<I>(args: I) -> std::result::Result<Action, UsageError>
where
    I: IntoIterator<Item = OsString>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    let Some((first, rest)) = args.split_first() else {
        return usage("missing subcommand", top_help());
    };

    match first.to_str() {
        Some("-h" | "--help") => Ok(Action::Help(top_help())),
        Some("-V" | "--version") => Ok(Action::Version(version())),
        Some("cat") => parse_cat(rest),
        Some("head") => parse_counted("head", |lines| Mode::Head { lines }, rest),
        Some("tail") => parse_counted("tail", |lines| Mode::Tail { lines }, rest),
        Some("schema") => parse_schema(rest),
        Some(other) => usage(format!("unknown subcommand `{other}`"), top_help()),
        None => usage("subcommand must be valid UTF-8", top_help()),
    }
}

fn parse_cat(args: &[OsString]) -> std::result::Result<Action, UsageError> {
    if contains_help(args) {
        return Ok(Action::Help(cat_help()));
    }

    let parsed = parse_common("cat", args, cat_help())?;
    Ok(Action::Run(Command {
        mode: Mode::Cat,
        output_format: parsed.output_format,
        file: parsed.file,
    }))
}

fn parse_schema(args: &[OsString]) -> std::result::Result<Action, UsageError> {
    if contains_help(args) {
        return Ok(Action::Help(schema_help()));
    }

    Ok(Action::Run(Command {
        mode: Mode::Schema,
        output_format: None,
        file: parse_file_only("schema", args, schema_help())?,
    }))
}

fn parse_counted(
    name: &'static str,
    mode: fn(usize) -> Mode,
    args: &[OsString],
) -> std::result::Result<Action, UsageError> {
    if contains_help(args) {
        return Ok(Action::Help(counted_help(name)));
    }

    let mut lines = 10usize;
    let mut common = CommonArgs::default();
    let mut i = 0;
    let mut flags_done = false;

    while i < args.len() {
        let arg = &args[i];
        if flags_done {
            common.positional.push(arg.clone());
            i += 1;
            continue;
        }

        match arg.to_str() {
            Some("--") => {
                flags_done = true;
                i += 1;
            }
            Some("-n") | Some("--lines") => {
                let Some(value) = args.get(i + 1) else {
                    return usage(
                        format!("`{}` requires a value", arg.to_string_lossy()),
                        counted_help(name),
                    );
                };
                lines = parse_lines(value, counted_help(name))?;
                i += 2;
            }
            Some("-j") | Some("--jsonl") => {
                common.set_output_format(OutputFormat::Jsonl, counted_help(name))?;
                i += 1;
            }
            Some("-t") | Some("--table") => {
                common.set_output_format(OutputFormat::Table, counted_help(name))?;
                i += 1;
            }
            Some(value) if value.starts_with("-n=") => {
                lines = parse_lines(OsStr::new(&value[3..]), counted_help(name))?;
                i += 1;
            }
            Some(value) if value.starts_with("--lines=") => {
                lines = parse_lines(OsStr::new(&value[8..]), counted_help(name))?;
                i += 1;
            }
            Some(value) if value.starts_with('-') => {
                return usage(format!("unsupported option `{value}`"), counted_help(name));
            }
            _ => {
                common.positional.push(arg.clone());
                i += 1;
            }
        }
    }

    let file = one_positional(name, common.positional, counted_help(name))?;
    Ok(Action::Run(Command {
        mode: mode(lines),
        output_format: common.output_format,
        file,
    }))
}

#[derive(Debug, Default)]
struct CommonArgs {
    output_format: Option<OutputFormat>,
    positional: Vec<OsString>,
}

impl CommonArgs {
    fn set_output_format(
        &mut self,
        format: OutputFormat,
        help: String,
    ) -> std::result::Result<(), UsageError> {
        if let Some(existing) = self.output_format
            && existing != format
        {
            return usage("choose either JSONL or table output, not both", help);
        }
        self.output_format = Some(format);
        Ok(())
    }
}

struct ParsedCommon {
    output_format: Option<OutputFormat>,
    file: PathBuf,
}

fn parse_common(
    name: &'static str,
    args: &[OsString],
    help: String,
) -> std::result::Result<ParsedCommon, UsageError> {
    let mut common = CommonArgs::default();
    let mut flags_done = false;

    for arg in args {
        if flags_done {
            common.positional.push(arg.clone());
            continue;
        }

        match arg.to_str() {
            Some("--") => flags_done = true,
            Some("-j") | Some("--jsonl") => {
                common.set_output_format(OutputFormat::Jsonl, help.clone())?
            }
            Some("-t") | Some("--table") => {
                common.set_output_format(OutputFormat::Table, help.clone())?
            }
            Some(value) if value.starts_with('-') && value != "-" => {
                return usage(format!("unsupported option `{value}`"), help);
            }
            _ => common.positional.push(arg.clone()),
        }
    }

    Ok(ParsedCommon {
        output_format: common.output_format,
        file: one_positional(name, common.positional, help)?,
    })
}

fn parse_file_only(
    name: &'static str,
    args: &[OsString],
    help: String,
) -> std::result::Result<PathBuf, UsageError> {
    let mut positional = Vec::new();
    let mut flags_done = false;

    for arg in args {
        if flags_done {
            positional.push(arg.clone());
            continue;
        }

        match arg.to_str() {
            Some("--") => flags_done = true,
            Some(value) if value.starts_with('-') && value != "-" => {
                return usage(format!("unsupported option `{value}`"), help);
            }
            _ => positional.push(arg.clone()),
        }
    }

    one_positional(name, positional, help)
}

fn one_positional(
    name: &'static str,
    positional: Vec<OsString>,
    help: String,
) -> std::result::Result<PathBuf, UsageError> {
    match positional.as_slice() {
        [] => usage(format!("`{name}` requires exactly one file path"), help),
        [file] => Ok(PathBuf::from(file)),
        _ => usage(format!("`{name}` accepts exactly one file path"), help),
    }
}

fn parse_lines(value: &OsStr, help: String) -> std::result::Result<usize, UsageError> {
    let Some(text) = value.to_str() else {
        return usage("line count must be valid UTF-8", help);
    };

    if text.is_empty() || text.starts_with('+') || text.starts_with('-') {
        return usage("line count must be a non-negative integer", help);
    }

    text.parse::<usize>().map_err(|_| UsageError {
        message: "line count must be a non-negative integer".to_string(),
        help,
    })
}

fn contains_help(args: &[OsString]) -> bool {
    let mut flags_done = false;
    for arg in args {
        if flags_done {
            continue;
        }
        match arg.to_str() {
            Some("--") => flags_done = true,
            Some("-h" | "--help") => return true,
            _ => {}
        }
    }
    false
}

fn usage<T>(message: impl Into<String>, help: String) -> std::result::Result<T, UsageError> {
    Err(UsageError {
        message: message.into(),
        help,
    })
}

fn version() -> String {
    format!("parqcat {}", env!("CARGO_PKG_VERSION"))
}

fn top_help() -> String {
    format!(
        "\
parqcat {}

Usage:
  parqcat cat <FILE>
  parqcat head [-n <N>] <FILE>
  parqcat tail [-n <N>] <FILE>
  parqcat schema <FILE>

Commands:
  cat       Emit every row
  head      Emit the first N rows
  tail      Emit the last N rows
  schema    Show the logical schema

Row output options:
  -j, --jsonl      Force compact JSONL output
  -t, --table      Force tabular output

Options:
  -h, --help       Show help
  -V, --version    Show version
",
        env!("CARGO_PKG_VERSION")
    )
}

fn cat_help() -> String {
    "\
Usage:
  parqcat cat <FILE>

Options:
  -j, --jsonl    Force compact JSONL output
  -t, --table    Force tabular output
  -h, --help     Show help
"
    .to_string()
}

fn schema_help() -> String {
    "\
Usage:
  parqcat schema <FILE>

Options:
  -h, --help     Show help
"
    .to_string()
}

fn counted_help(name: &str) -> String {
    format!(
        "\
Usage:
  parqcat {name} <FILE>
  parqcat {name} -n <N> <FILE>
  parqcat {name} --lines <N> <FILE>

Options:
  -n, --lines <N>    Number of rows to emit, default 10
  -j, --jsonl        Force compact JSONL output
  -t, --table        Force tabular output
  -h, --help         Show help
"
    )
}
