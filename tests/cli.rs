use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;

use arrow_array::types::Int32Type;
use arrow_array::{
    Array, ArrayRef, BinaryArray, BooleanArray, Date32Array, Decimal128Array, DictionaryArray,
    Float64Array, Int32Array, ListArray, RecordBatch, StringArray, StructArray,
    Time64MicrosecondArray, TimestampMillisecondArray,
};
use arrow_schema::{DataType, Field, Schema};
use flate2::Compression as GzipCompression;
use flate2::write::GzEncoder;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use tempfile::tempdir;

fn parqcat() -> Command {
    Command::new(env!("CARGO_BIN_EXE_parqcat"))
}

#[test]
fn cat_head_and_tail_flat_jsonl() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("flat.parquet");
    write_flat_fixture(&path);

    let expected = [
        r#"{"id":1,"name":"alpha","active":true,"score":1.5,"payload":"base64:AAEC"}"#,
        r#"{"id":2,"name":"beta","active":false,"score":"NaN","payload":"base64:aGk="}"#,
        r#"{"id":3,"name":null,"active":true,"score":"+Inf","payload":"base64:/w=="}"#,
        r#"{"id":4,"name":"delta","active":null,"score":"-Inf","payload":"base64:"}"#,
        r#"{"id":5,"name":"epsilon","active":true,"score":null,"payload":"base64:AP8="}"#,
    ]
    .join("\n")
        + "\n";

    assert_success(parqcat().arg("cat").arg(&path).output().unwrap(), &expected);

    assert_success(
        parqcat().args(["cat", "-j"]).arg(&path).output().unwrap(),
        &expected,
    );

    assert_success(
        parqcat()
            .args(["head", "-n", "2"])
            .arg(&path)
            .output()
            .unwrap(),
        &(expected.lines().take(2).collect::<Vec<_>>().join("\n") + "\n"),
    );

    assert_success(
        parqcat()
            .args(["tail", "--lines=2"])
            .arg(&path)
            .output()
            .unwrap(),
        &(expected.lines().skip(3).collect::<Vec<_>>().join("\n") + "\n"),
    );

    assert_success(
        parqcat()
            .args(["head", "-n", "0"])
            .arg(&path)
            .output()
            .unwrap(),
        "",
    );

    assert_success(
        parqcat()
            .args(["tail", "-n", "99"])
            .arg(&path)
            .output()
            .unwrap(),
        &expected,
    );
}

#[test]
fn table_output_is_available_for_quick_terminal_views() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("flat.parquet");
    write_flat_fixture(&path);

    let output = parqcat()
        .args(["head", "-t", "-n", "2"])
        .arg(&path)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr:\n{}", stderr(&output));
    assert_eq!(stderr(&output), "");
    let stdout = stdout(&output);
    assert!(stdout.starts_with("id        name      active    score     payload"));
    assert!(stdout.contains("--------  --------  --------  --------  --------"));
    assert!(stdout.contains("1         alpha     true      1.5       base6..."));
    assert!(stdout.contains("2         beta      false     NaN       base6..."));
    assert!(!stdout.starts_with('{'));
}

#[test]
fn schema_subcommand_shows_logical_schema() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("flat.parquet");
    let nested_path = dir.path().join("nested.parquet");
    write_flat_fixture(&path);
    write_nested_fixture(&nested_path);

    let output = parqcat().arg("schema").arg(&path).output().unwrap();

    assert!(output.status.success(), "stderr:\n{}", stderr(&output));
    assert_eq!(stderr(&output), "");
    let flat_stdout = stdout(&output);
    let rows = flat_stdout
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .collect::<Vec<_>>();

    assert_eq!(rows[0], ["name", "type", "nullable"]);
    assert_eq!(rows[2], ["id", "Int32", "no"]);
    assert_eq!(rows[3], ["name", "Utf8", "yes"]);
    assert_eq!(rows[4], ["active", "Boolean", "yes"]);
    assert_eq!(rows[5], ["score", "Float64", "yes"]);
    assert_eq!(rows[6], ["payload", "Binary", "yes"]);

    let nested = parqcat().arg("schema").arg(&nested_path).output().unwrap();
    assert!(nested.status.success(), "stderr:\n{}", stderr(&nested));
    assert_eq!(stderr(&nested), "");
    let nested_stdout = stdout(&nested);
    assert!(
        nested_stdout
            .lines()
            .any(|line| line.starts_with("tags") && line.contains("List"))
    );
    assert!(
        nested_stdout
            .lines()
            .any(|line| line.starts_with("  item") && line.contains("Int32"))
    );
    assert!(
        nested_stdout
            .lines()
            .any(|line| line.starts_with("profile") && line.contains("Struct"))
    );
    assert!(
        nested_stdout
            .lines()
            .any(|line| line.starts_with("  city") && line.contains("Utf8"))
    );
}

#[test]
fn nested_and_dictionary_jsonl() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested.parquet");
    write_nested_fixture(&path);

    let expected = [
        r#"{"id":1,"category":"alpha","tags":[1,2],"profile":{"city":"NYC","zip":10001}}"#,
        r#"{"id":2,"category":"beta","tags":[],"profile":{"city":null,"zip":null}}"#,
        r#"{"id":3,"category":"alpha","tags":null,"profile":{"city":"LA","zip":90001}}"#,
    ]
    .join("\n")
        + "\n";

    assert_success(parqcat().arg("cat").arg(&path).output().unwrap(), &expected);
}

#[test]
fn gzip_and_zstd_outer_wrappers_are_read_transparently() {
    let dir = tempdir().unwrap();
    let parquet_path = dir.path().join("flat.parquet");
    let gzip_path = dir.path().join("flat.parquet.gz");
    let zstd_path = dir.path().join("flat.parquet.zst");
    write_flat_fixture(&parquet_path);
    gzip_file(&parquet_path, &gzip_path);
    zstd_file(&parquet_path, &zstd_path);

    let expected = r#"{"id":5,"name":"epsilon","active":true,"score":null,"payload":"base64:AP8="}"#
        .to_string() + "\n";

    assert_success(
        parqcat()
            .args(["tail", "-n", "1"])
            .arg(&gzip_path)
            .output()
            .unwrap(),
        &expected,
    );
    assert_success(
        parqcat()
            .args(["tail", "-n", "1"])
            .arg(&zstd_path)
            .output()
            .unwrap(),
        &expected,
    );
}

#[test]
fn temporal_and_decimal_values_have_stable_json_strings() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("temporal.parquet");
    write_temporal_fixture(&path);

    let expected = [
        r#"{"date":"1970-01-02","time":"12:34:56.789000","ts_utc":"1970-01-01T00:00:01.000Z","ts_naive":"1970-01-01T00:00:01.000","amount":"123.45"}"#,
        r#"{"date":null,"time":null,"ts_utc":null,"ts_naive":null,"amount":"-6.78"}"#,
    ]
    .join("\n")
        + "\n";

    assert_success(parqcat().arg("cat").arg(&path).output().unwrap(), &expected);

    let table = parqcat()
        .args(["head", "-t", "-n", "1"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(table.status.success(), "stderr:\n{}", stderr(&table));
    assert_eq!(stderr(&table), "");
    let stdout = stdout(&table);
    assert!(stdout.contains("1970-01-02"));
    assert!(stdout.contains("12:34:56.789000"));
    assert!(stdout.contains("1970-01-01T00:00:01.000Z"));
    assert!(stdout.contains("1970-01-01T00:00:01.000"));
    assert!(!stdout.contains("..."));
}

#[test]
fn output_streams_cleanly_into_jq() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("flat.parquet");
    write_flat_fixture(&path);

    let mut producer = parqcat()
        .arg("cat")
        .arg(&path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = producer.stdout.take().unwrap();

    let jq = Command::new("jq")
        .args(["-r", r#".name // "NULL""#])
        .stdin(Stdio::from(stdout))
        .output()
        .unwrap();
    let producer = producer.wait_with_output().unwrap();

    assert_success(producer, "");
    assert_success(jq, "alpha\nbeta\nNULL\ndelta\nepsilon\n");
}

#[test]
fn usage_and_runtime_errors_have_expected_exit_codes() {
    let help = parqcat().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(stdout(&help).contains("Usage:"));
    assert!(stdout(&help).contains("schema"));
    assert_eq!(stderr(&help), "");

    let version = parqcat().arg("--version").output().unwrap();
    assert!(version.status.success());
    assert!(stdout(&version).starts_with("parqcat "));
    assert_eq!(stderr(&version), "");

    let no_args = parqcat().output().unwrap();
    assert_eq!(no_args.status.code(), Some(2));
    assert!(stderr(&no_args).contains("missing subcommand"));

    let signed_count = parqcat()
        .args(["head", "-n", "-1", "file.parquet"])
        .output()
        .unwrap();
    assert_eq!(signed_count.status.code(), Some(2));
    assert!(stderr(&signed_count).contains("non-negative integer"));

    let stdin = parqcat().args(["cat", "-"]).output().unwrap();
    assert_eq!(stdin.status.code(), Some(1));
    assert!(stderr(&stdin).contains("stdin is not supported"));

    let schema_format = parqcat()
        .args(["schema", "-j", "file.parquet"])
        .output()
        .unwrap();
    assert_eq!(schema_format.status.code(), Some(2));
    assert!(stderr(&schema_format).contains("unsupported option `-j`"));

    let dir = tempdir().unwrap();
    let corrupt_path = dir.path().join("corrupt.parquet");
    std::fs::write(&corrupt_path, b"not parquet").unwrap();
    let corrupt = parqcat().arg("cat").arg(corrupt_path).output().unwrap();
    assert_eq!(corrupt.status.code(), Some(1));
}

fn assert_success(output: Output, expected_stdout: &str) {
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(stdout(&output), expected_stdout);
    assert_eq!(stderr(&output), "");
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn write_flat_fixture(path: &Path) {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int32, false),
        Field::new("name", DataType::Utf8, true),
        Field::new("active", DataType::Boolean, true),
        Field::new("score", DataType::Float64, true),
        Field::new("payload", DataType::Binary, true),
    ]));

    let payload: BinaryArray = vec![
        Some(&b"\x00\x01\x02"[..]),
        Some(&b"hi"[..]),
        Some(&b"\xff"[..]),
        Some(&b""[..]),
        Some(&b"\x00\xff"[..]),
    ]
    .into_iter()
    .collect();

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int32Array::from(vec![1, 2, 3, 4, 5])) as ArrayRef,
            Arc::new(StringArray::from(vec![
                Some("alpha"),
                Some("beta"),
                None,
                Some("delta"),
                Some("epsilon"),
            ])),
            Arc::new(BooleanArray::from(vec![
                Some(true),
                Some(false),
                Some(true),
                None,
                Some(true),
            ])),
            Arc::new(Float64Array::from(vec![
                Some(1.5),
                Some(f64::NAN),
                Some(f64::INFINITY),
                Some(f64::NEG_INFINITY),
                None,
            ])),
            Arc::new(payload),
        ],
    )
    .unwrap();

    write_parquet(path, schema, &[batch]);
}

fn write_nested_fixture(path: &Path) {
    let id = Arc::new(Int32Array::from(vec![1, 2, 3])) as ArrayRef;
    let category_array: DictionaryArray<Int32Type> =
        vec![Some("alpha"), Some("beta"), Some("alpha")]
            .into_iter()
            .collect();
    let category = Arc::new(category_array) as ArrayRef;
    let tags = Arc::new(ListArray::from_iter_primitive::<Int32Type, _, _>([
        Some(vec![Some(1), Some(2)]),
        Some(vec![]),
        None,
    ])) as ArrayRef;
    let profile = Arc::new(StructArray::from(vec![
        (
            Arc::new(Field::new("city", DataType::Utf8, true)),
            Arc::new(StringArray::from(vec![Some("NYC"), None, Some("LA")])) as ArrayRef,
        ),
        (
            Arc::new(Field::new("zip", DataType::Int32, true)),
            Arc::new(Int32Array::from(vec![Some(10001), None, Some(90001)])) as ArrayRef,
        ),
    ])) as ArrayRef;

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", id.data_type().clone(), false),
        Field::new("category", category.data_type().clone(), true),
        Field::new("tags", tags.data_type().clone(), true),
        Field::new("profile", profile.data_type().clone(), true),
    ]));

    let batch = RecordBatch::try_new(schema.clone(), vec![id, category, tags, profile]).unwrap();
    write_parquet(path, schema, &[batch]);
}

fn write_temporal_fixture(path: &Path) {
    let date = Arc::new(Date32Array::from(vec![Some(1), None])) as ArrayRef;
    let time = Arc::new(Time64MicrosecondArray::from(vec![
        Some(45_296_789_000),
        None,
    ])) as ArrayRef;
    let ts_utc =
        Arc::new(TimestampMillisecondArray::from(vec![Some(1_000), None]).with_timezone_utc())
            as ArrayRef;
    let ts_naive = Arc::new(TimestampMillisecondArray::from(vec![Some(1_000), None])) as ArrayRef;
    let amount = Arc::new(
        Decimal128Array::from(vec![Some(12_345), Some(-678)])
            .with_precision_and_scale(10, 2)
            .unwrap(),
    ) as ArrayRef;

    let schema = Arc::new(Schema::new(vec![
        Field::new("date", date.data_type().clone(), true),
        Field::new("time", time.data_type().clone(), true),
        Field::new("ts_utc", ts_utc.data_type().clone(), true),
        Field::new("ts_naive", ts_naive.data_type().clone(), true),
        Field::new("amount", amount.data_type().clone(), true),
    ]));

    let batch =
        RecordBatch::try_new(schema.clone(), vec![date, time, ts_utc, ts_naive, amount]).unwrap();
    write_parquet(path, schema, &[batch]);
}

fn write_parquet(path: &Path, schema: Arc<Schema>, batches: &[RecordBatch]) {
    let file = File::create(path).unwrap();
    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .set_dictionary_enabled(true)
        .set_max_row_group_row_count(Some(2))
        .build();
    let mut writer = ArrowWriter::try_new(file, schema, Some(props)).unwrap();
    for batch in batches {
        writer.write(batch).unwrap();
    }
    writer.close().unwrap();
}

fn gzip_file(input: &Path, output: &Path) {
    let mut encoder = GzEncoder::new(File::create(output).unwrap(), GzipCompression::default());
    let bytes = std::fs::read(input).unwrap();
    encoder.write_all(&bytes).unwrap();
    encoder.finish().unwrap();
}

fn zstd_file(input: &Path, output: &Path) {
    let bytes = std::fs::read(input).unwrap();
    let compressed = zstd::stream::encode_all(bytes.as_slice(), 3).unwrap();
    std::fs::write(output, compressed).unwrap();
}
