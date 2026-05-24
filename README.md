# parqcat

[![CI](https://github.com/r0k3/parqcat/actions/workflows/ci.yml/badge.svg)](https://github.com/r0k3/parqcat/actions/workflows/ci.yml)

`parqcat` is a lightweight command-line utility for inspecting local Parquet files.

Parquet is a good storage format, but quick inspection often means starting Python, a JVM tool, a SQL shell, or a larger data system. `parqcat` is meant to cover the everyday terminal workflow: show rows, preview the beginning or end of a file, inspect the schema, and pipe rows into tools such as `jq`.

## Install

From crates.io:

```text
cargo install parqcat
```

From this repository:

```text
cargo install --path .
```

For development:

```text
cargo build
cargo test
```

## Features

- Native Rust CLI with no Python or JVM runtime dependency.
- `cat`, `head`, and `tail` commands for row inspection.
- `schema` command for logical schema inspection without decoding row data.
- Table output for quick terminal viewing.
- JSONL output for scripts and pipelines.
- Transparent support for `.gz`, `.zst`, and `.zstd` outer-compressed Parquet files.
- Support for common Parquet primitive, nested, dictionary, temporal, decimal, and binary values.

## Usage

```text
parqcat cat <FILE>
parqcat head [-n <N>] <FILE>
parqcat tail [-n <N>] <FILE>
parqcat schema <FILE>
```

Examples:
Author: Robert Kende

```text
parqcat cat data.parquet
parqcat head -n 20 events.parquet
parqcat tail --lines 5 archive/events.parquet.zstd
parqcat schema data.parquet
```

When stdout is a terminal, row commands default to compact table output. When stdout is piped or redirected, row commands default to compact newline-delimited JSON.

```text
parqcat cat events.parquet | jq -r '.event_type'
parqcat head -n 100 events.parquet | jq 'select(.status == "failed")'
```

Force JSONL with `-j` or `--jsonl`:

```text
parqcat cat -j events.parquet
```

Force table output with `-t` or `--table`:

```text
parqcat head -t -n 20 events.parquet
```

## Output

JSONL output is one compact JSON object per logical row. It writes only data to stdout; diagnostics go to stderr.

Table output is intended for humans. It is compact, deterministic, uncolored, and may truncate very wide values. Date, time, and timestamp columns reserve enough width for common rendered values.

Schema output is a compact table:

```text
name      type     nullable
--------  -------  --------
id        Int32    no
name      Utf8     yes
```

Nested schema fields are indented below their parent field.

## Supported Input

`parqcat` targets mainstream, unencrypted local Parquet files.

Supported:

- local file paths
- common primitive types
- nullable fields
- dictionary encoding
- nested lists, structs, and maps
- common Parquet internal compression codecs
- outer `.gz`, `.zst`, and `.zstd` wrappers

Not supported in v1:

- stdin
- remote object stores
- multiple input files
- SQL, filtering, projection, or file mutation
- CSV output
- encrypted Parquet files requiring keys

## Development

Run the local checks before opening a change:

```text
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Tests generate compact Parquet fixtures in Rust; no Python, JVM, or external data engine is required.

## License

Apache-2.0. See [LICENSE](LICENSE).
