# parqcat

**`cat` for Parquet files.** Peek at rows, check the schema, and pipe data into `jq`, all from one small native binary. No Python, JVM, or SQL shell required.

[![CI](https://github.com/r0k3/parqcat/actions/workflows/ci.yml/badge.svg)](https://github.com/r0k3/parqcat/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/parqcat.svg)](https://crates.io/crates/parqcat)
[![MSRV](https://img.shields.io/crates/msrv/parqcat.svg)](#install)
[![License](https://img.shields.io/crates/l/parqcat.svg)](LICENSE)

```console
$ parqcat head -n 5 events.parquet
id    ts                        user      event_type  status  amount  tags
----  ------------------------  --------  ----------  ------  ------  -------
1000  2025-10-09T08:53:20.000Z  ada       page_view   failed  null    []
1001  2025-10-09T08:53:57.892Z  margaret  refund      ok      -24.21  ["vip"]
1002  2025-10-09T08:54:34.884Z  dennis    refund      ok      -46.43  null
1003  2025-10-09T08:55:11.877Z  linus     page_view   ok      null    ["vip"]
1004  2025-10-09T08:55:48.869Z  barbara   page_view   ok      null    null
```

```console
$ parqcat schema events.parquet
name        type                    nullable
----------  ----------------------  --------
id          Int64                   no
ts          Timestamp(ms, "UTC")    no
user        Utf8                    no
event_type  Dictionary(Int8, Utf8)  no
status      Utf8                    no
amount      Decimal128(10, 2)       yes
tags        List                    yes
  item      Utf8                    yes
```

```console
$ parqcat cat events.parquet | jq -r 'select(.event_type == "checkout") | .user' | sort | uniq -c | sort -rn | head -3
     65 grace
     64 frances
     63 margaret
```

## Why parqcat?

Parquet is a great storage format, but taking a quick look at a file usually means starting Python, a JVM tool, or a SQL engine. `parqcat` covers the everyday terminal workflow instead:

- **One native binary.** Install it once and it starts instantly. It needs no runtime.
- **Works for people and pipelines.** At a terminal you get a readable table. When the output is piped or redirected, you get newline-delimited JSON, ready for `jq`, `grep`, or a script.
- **Understands real-world data.** It handles nested lists, structs, and maps, as well as dictionary-encoded columns, decimals, dates, times, timestamps, and binary values.
- **Fits Unix habits.** The commands are `cat`, `head`, `tail`, and `schema`. Data goes to stdout and diagnostics go to stderr.

## Install

With Homebrew (macOS and Linux):

```sh
brew install r0k3/tap/parqcat
```

With the prebuilt installer (macOS and Linux):

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/r0k3/parqcat/releases/latest/download/parqcat-installer.sh | sh
```

From crates.io:

```sh
cargo install --locked parqcat
```

From this repository:

```sh
cargo install --locked --path .
```

`--locked` builds with the exact dependency versions `parqcat` is tested against.

Building requires Rust 1.97 or newer. `parqcat` supports the latest stable Rust release and the two before it; the minimum version is raised as new releases come out.

## Usage

```text
parqcat cat <FILE>              every row
parqcat head [-n <N>] <FILE>    the first N rows (default 10)
parqcat tail [-n <N>] <FILE>    the last N rows (default 10)
parqcat schema <FILE>           the logical schema, without decoding row data
```

Row commands choose their format automatically: a compact table when stdout is a terminal, and JSONL when it is piped or redirected. You can override this with `-t`/`--table` or `-j`/`--jsonl`:

```console
$ parqcat head -j -n 2 events.parquet
{"id":1000,"ts":"2025-10-09T08:53:20.000Z","user":"ada","event_type":"page_view","status":"failed","amount":null,"tags":[]}
{"id":1001,"ts":"2025-10-09T08:53:57.892Z","user":"margaret","event_type":"refund","status":"ok","amount":"-24.21","tags":["vip"]}
```

## Recipes

Find failed events and keep only the fields you care about:

```console
$ parqcat cat events.parquet | jq -c 'select(.status == "failed") | {id, user, event_type}' | head -3
{"id":1000,"user":"ada","event_type":"page_view"}
{"id":1008,"user":"margaret","event_type":"search"}
{"id":1015,"user":"linus","event_type":"checkout"}
```

Pretty-print the last row:

```console
$ parqcat tail -n 1 events.parquet | jq .
{
  "id": 2999,
  "ts": "2025-10-10T05:26:29.173Z",
  "user": "linus",
  "event_type": "page_view",
  "status": "ok",
  "amount": null,
  "tags": null
}
```

Count the rows:

```console
$ parqcat cat events.parquet | wc -l
2000
```

## Output

**JSONL** has one compact JSON object per row. Decimals are written as strings so no precision is lost. Timestamps use RFC 3339 format, and binary values are written as `base64:...` strings.

**Tables** are meant for people. They are compact, deterministic, and uncolored. Each column is sized to fit its header and the first 8,192 rows, up to 32 characters; longer values are truncated. Nested values are shown as JSON. Date, time, and timestamp columns reserve enough width for common rendered values. In schema output, nested fields are indented below their parent field.

<details>
<summary><strong>Supported input</strong></summary>

`parqcat` targets mainstream, unencrypted local Parquet files.

Supported:

- local file paths
- common primitive types
- nullable fields
- dictionary encoding
- nested lists, structs, and maps
- common Parquet internal compression codecs

Not supported in v1:

- stdin
- remote object stores
- multiple input files
- outer compression wrappers such as `.gz`, `.zst`, and `.zstd`
- SQL, filtering, projection, or file mutation
- CSV output
- encrypted Parquet files requiring keys

</details>

## Development

Run the local checks before opening a change:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Tests generate compact Parquet fixtures in Rust; no Python, JVM, or external data engine is required.

## License

Apache-2.0. See [LICENSE](LICENSE).
