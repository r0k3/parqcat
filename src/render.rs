use std::collections::HashSet;
use std::io::Write;

use arrow_array::types::*;
use arrow_array::{
    Array, BinaryArray, BinaryViewArray, BooleanArray, Date32Array, Date64Array, Decimal32Array,
    Decimal64Array, Decimal128Array, Decimal256Array, DictionaryArray, FixedSizeBinaryArray,
    FixedSizeListArray, Float16Array, Float32Array, Float64Array, Int8Array, Int16Array,
    Int32Array, Int64Array, LargeBinaryArray, LargeListArray, LargeListViewArray, LargeStringArray,
    ListArray, ListViewArray, MapArray, RecordBatch, StringArray, StringViewArray, StructArray,
    Time32MillisecondArray, Time32SecondArray, Time64MicrosecondArray, Time64NanosecondArray,
    TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
    TimestampSecondArray, UInt8Array, UInt16Array, UInt32Array, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema, TimeUnit};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chrono::{DateTime, Utc};

use crate::error::{Error, Result};

const MIN_TABLE_COLUMN_WIDTH: usize = 8;
const MAX_TABLE_COLUMN_WIDTH: usize = 32;

pub fn write_json_batch(batch: &RecordBatch, output: &mut impl Write) -> Result<()> {
    let fields = batch.schema().fields().clone();

    for row in 0..batch.num_rows() {
        output.write_all(b"{")?;
        for (column_index, field) in fields.iter().enumerate() {
            if column_index > 0 {
                output.write_all(b",")?;
            }
            write_json_str(output, field.name())?;
            output.write_all(b":")?;
            write_value(
                batch.column(column_index).as_ref(),
                field.data_type(),
                row,
                output,
            )?;
        }
        output.write_all(b"}\n")?;
    }

    Ok(())
}

pub struct TableWriter {
    fields: Vec<Field>,
    widths: Vec<usize>,
    wrote_header: bool,
}

impl TableWriter {
    pub fn new(batch: &RecordBatch) -> Self {
        let fields = batch
            .schema()
            .fields()
            .iter()
            .map(|field| field.as_ref().clone())
            .collect::<Vec<_>>();
        let widths = fields.iter().map(table_column_width).collect();

        Self {
            fields,
            widths,
            wrote_header: false,
        }
    }

    pub fn write_batch(&mut self, batch: &RecordBatch, output: &mut impl Write) -> Result<()> {
        if !self.wrote_header {
            let headers = self
                .fields
                .iter()
                .map(|field| field.name().as_str())
                .collect::<Vec<_>>();
            write_table_row(output, &headers, &self.widths)?;
            let separators = self
                .widths
                .iter()
                .map(|width| "-".repeat(*width))
                .collect::<Vec<_>>();
            let separator_refs = separators.iter().map(String::as_str).collect::<Vec<_>>();
            write_table_row(output, &separator_refs, &self.widths)?;
            self.wrote_header = true;
        }

        for row in 0..batch.num_rows() {
            for (column_index, field) in self.fields.iter().enumerate() {
                if column_index > 0 {
                    output.write_all(b"  ")?;
                }

                let display =
                    display_value(batch.column(column_index).as_ref(), field.data_type(), row)?;
                write_table_cell(output, &display, self.widths[column_index])?;
            }
            output.write_all(b"\n")?;
        }

        Ok(())
    }
}

pub fn write_schema(schema: &Schema, output: &mut impl Write) -> Result<()> {
    let mut rows = Vec::new();
    for field in schema.fields() {
        collect_schema_field(&mut rows, field.as_ref(), 0);
    }

    let name_width = schema_column_width("name", rows.iter().map(|row| row.name.as_str()));
    let type_width = schema_column_width("type", rows.iter().map(|row| row.data_type.as_str()));
    let nullable_width =
        schema_column_width("nullable", rows.iter().map(|row| row.nullable.as_str()));

    write_schema_row(output, "name", "type", "nullable", name_width, type_width)?;
    write_schema_row(
        output,
        &"-".repeat(name_width),
        &"-".repeat(type_width),
        &"-".repeat(nullable_width),
        name_width,
        type_width,
    )?;

    for row in rows {
        write_schema_row(
            output,
            &row.name,
            &row.data_type,
            &row.nullable,
            name_width,
            type_width,
        )?;
    }

    Ok(())
}

struct SchemaRow {
    name: String,
    data_type: String,
    nullable: String,
}

fn collect_schema_field(rows: &mut Vec<SchemaRow>, field: &Field, depth: usize) {
    rows.push(SchemaRow {
        name: format!("{}{}", "  ".repeat(depth), field.name()),
        data_type: schema_type_label(field.data_type()),
        nullable: if field.is_nullable() { "yes" } else { "no" }.to_string(),
    });

    match field.data_type() {
        DataType::List(child)
        | DataType::LargeList(child)
        | DataType::ListView(child)
        | DataType::LargeListView(child) => {
            collect_schema_field(rows, child.as_ref(), depth + 1);
        }
        DataType::FixedSizeList(child, _) => {
            collect_schema_field(rows, child.as_ref(), depth + 1);
        }
        DataType::Struct(fields) => {
            for child in fields {
                collect_schema_field(rows, child.as_ref(), depth + 1);
            }
        }
        DataType::Map(child, _) => {
            collect_schema_field(rows, child.as_ref(), depth + 1);
        }
        DataType::Union(fields, _) => {
            for (_, child) in fields.iter() {
                collect_schema_field(rows, child.as_ref(), depth + 1);
            }
        }
        DataType::RunEndEncoded(run_ends, values) => {
            collect_schema_field(rows, run_ends.as_ref(), depth + 1);
            collect_schema_field(rows, values.as_ref(), depth + 1);
        }
        _ => {}
    }
}

fn schema_type_label(data_type: &DataType) -> String {
    match data_type {
        DataType::List(_) => "List".to_string(),
        DataType::LargeList(_) => "LargeList".to_string(),
        DataType::ListView(_) => "ListView".to_string(),
        DataType::LargeListView(_) => "LargeListView".to_string(),
        DataType::FixedSizeList(_, size) => format!("FixedSizeList({size})"),
        DataType::Struct(_) => "Struct".to_string(),
        DataType::Map(_, sorted) => {
            if *sorted {
                "Map(sorted)".to_string()
            } else {
                "Map".to_string()
            }
        }
        DataType::Union(_, mode) => format!("Union({mode:?})"),
        DataType::RunEndEncoded(_, _) => "RunEndEncoded".to_string(),
        _ => data_type.to_string(),
    }
}

fn schema_column_width<'a>(header: &str, values: impl Iterator<Item = &'a str>) -> usize {
    values
        .map(display_width)
        .chain(std::iter::once(display_width(header)))
        .max()
        .unwrap_or(0)
}

fn write_schema_row(
    output: &mut impl Write,
    name: &str,
    data_type: &str,
    nullable: &str,
    name_width: usize,
    type_width: usize,
) -> Result<()> {
    write_padded(output, name, name_width)?;
    output.write_all(b"  ")?;
    write_padded(output, data_type, type_width)?;
    output.write_all(b"  ")?;
    output.write_all(nullable.as_bytes())?;
    output.write_all(b"\n")?;
    Ok(())
}

fn write_padded(output: &mut impl Write, cell: &str, width: usize) -> Result<()> {
    output.write_all(cell.as_bytes())?;
    for _ in 0..width.saturating_sub(display_width(cell)) {
        output.write_all(b" ")?;
    }
    Ok(())
}

fn table_column_width(field: &Field) -> usize {
    let header_width =
        display_width(field.name()).clamp(MIN_TABLE_COLUMN_WIDTH, MAX_TABLE_COLUMN_WIDTH);
    let data_width = temporal_table_width(field.data_type()).unwrap_or(MIN_TABLE_COLUMN_WIDTH);
    header_width.max(data_width).min(MAX_TABLE_COLUMN_WIDTH)
}

fn temporal_table_width(data_type: &DataType) -> Option<usize> {
    match data_type {
        DataType::Date32 | DataType::Date64 => Some(10),
        DataType::Time32(unit) | DataType::Time64(unit) => Some(time_table_width(*unit)),
        DataType::Timestamp(unit, timezone) => {
            Some(timestamp_table_width(*unit) + usize::from(timezone.is_some()))
        }
        _ => None,
    }
}

fn time_table_width(unit: TimeUnit) -> usize {
    match unit {
        TimeUnit::Second => 8,
        TimeUnit::Millisecond => 12,
        TimeUnit::Microsecond => 15,
        TimeUnit::Nanosecond => 18,
    }
}

fn timestamp_table_width(unit: TimeUnit) -> usize {
    match unit {
        TimeUnit::Second => 19,
        TimeUnit::Millisecond => 23,
        TimeUnit::Microsecond => 26,
        TimeUnit::Nanosecond => 29,
    }
}

fn display_value(array: &dyn Array, data_type: &DataType, row: usize) -> Result<String> {
    let mut json = Vec::new();
    write_value(array, data_type, row, &mut json)?;
    let json = String::from_utf8(json)
        .map_err(|err| Error::Unsupported(format!("rendered value was not valid UTF-8: {err}")))?;

    if json.starts_with('"') {
        Ok(serde_json::from_str::<String>(&json)?)
    } else {
        Ok(json)
    }
}

fn write_table_row(output: &mut impl Write, cells: &[&str], widths: &[usize]) -> Result<()> {
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            output.write_all(b"  ")?;
        }
        write_table_cell(output, cell, widths[index])?;
    }
    output.write_all(b"\n")?;
    Ok(())
}

fn write_table_cell(output: &mut impl Write, cell: &str, width: usize) -> Result<()> {
    let cell = normalize_cell(cell);
    let cell = truncate_cell(&cell, width);
    let padding = width.saturating_sub(display_width(&cell));
    output.write_all(cell.as_bytes())?;
    for _ in 0..padding {
        output.write_all(b" ")?;
    }
    Ok(())
}

fn normalize_cell(cell: &str) -> String {
    cell.chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect()
}

fn truncate_cell(cell: &str, width: usize) -> String {
    if display_width(cell) <= width {
        return cell.to_string();
    }

    if width <= 3 {
        return ".".repeat(width);
    }

    let mut truncated = String::new();
    for ch in cell.chars().take(width - 3) {
        truncated.push(ch);
    }
    truncated.push_str("...");
    truncated
}

fn display_width(value: &str) -> usize {
    value.chars().count()
}

fn write_value(
    array: &dyn Array,
    data_type: &DataType,
    row: usize,
    output: &mut impl Write,
) -> Result<()> {
    if array.is_null(row) {
        output.write_all(b"null")?;
        return Ok(());
    }

    match data_type {
        DataType::Null => output.write_all(b"null")?,
        DataType::Boolean => write_bool(output, downcast::<BooleanArray>(array)?.value(row))?,
        DataType::Int8 => write!(output, "{}", downcast::<Int8Array>(array)?.value(row))?,
        DataType::Int16 => write!(output, "{}", downcast::<Int16Array>(array)?.value(row))?,
        DataType::Int32 => write!(output, "{}", downcast::<Int32Array>(array)?.value(row))?,
        DataType::Int64 => write!(output, "{}", downcast::<Int64Array>(array)?.value(row))?,
        DataType::UInt8 => write!(output, "{}", downcast::<UInt8Array>(array)?.value(row))?,
        DataType::UInt16 => write!(output, "{}", downcast::<UInt16Array>(array)?.value(row))?,
        DataType::UInt32 => write!(output, "{}", downcast::<UInt32Array>(array)?.value(row))?,
        DataType::UInt64 => write!(output, "{}", downcast::<UInt64Array>(array)?.value(row))?,
        DataType::Float16 => write_float(
            output,
            f64::from(downcast::<Float16Array>(array)?.value(row).to_f32()),
        )?,
        DataType::Float32 => write_float(
            output,
            f64::from(downcast::<Float32Array>(array)?.value(row)),
        )?,
        DataType::Float64 => write_float(output, downcast::<Float64Array>(array)?.value(row))?,
        DataType::Utf8 => write_json_str(output, downcast::<StringArray>(array)?.value(row))?,
        DataType::LargeUtf8 => {
            write_json_str(output, downcast::<LargeStringArray>(array)?.value(row))?
        }
        DataType::Utf8View => {
            write_json_str(output, downcast::<StringViewArray>(array)?.value(row))?
        }
        DataType::Binary => write_binary(output, downcast::<BinaryArray>(array)?.value(row))?,
        DataType::LargeBinary => {
            write_binary(output, downcast::<LargeBinaryArray>(array)?.value(row))?
        }
        DataType::BinaryView => {
            write_binary(output, downcast::<BinaryViewArray>(array)?.value(row))?
        }
        DataType::FixedSizeBinary(_) => {
            write_binary(output, downcast::<FixedSizeBinaryArray>(array)?.value(row))?
        }
        DataType::Decimal32(_, _) => write_json_str(
            output,
            &downcast::<Decimal32Array>(array)?.value_as_string(row),
        )?,
        DataType::Decimal64(_, _) => write_json_str(
            output,
            &downcast::<Decimal64Array>(array)?.value_as_string(row),
        )?,
        DataType::Decimal128(_, _) => write_json_str(
            output,
            &downcast::<Decimal128Array>(array)?.value_as_string(row),
        )?,
        DataType::Decimal256(_, _) => write_json_str(
            output,
            &downcast::<Decimal256Array>(array)?.value_as_string(row),
        )?,
        DataType::Date32 => write_json_str(
            output,
            &format_date32(downcast::<Date32Array>(array)?.value(row))?,
        )?,
        DataType::Date64 => write_json_str(
            output,
            &format_date64(downcast::<Date64Array>(array)?.value(row))?,
        )?,
        DataType::Time32(unit) => match unit {
            TimeUnit::Second => write_json_str(
                output,
                &format_time(
                    downcast::<Time32SecondArray>(array)?.value(row) as i64,
                    TimeUnit::Second,
                ),
            )?,
            TimeUnit::Millisecond => write_json_str(
                output,
                &format_time(
                    downcast::<Time32MillisecondArray>(array)?.value(row) as i64,
                    TimeUnit::Millisecond,
                ),
            )?,
            _ => return unsupported_type(data_type),
        },
        DataType::Time64(unit) => match unit {
            TimeUnit::Microsecond => write_json_str(
                output,
                &format_time(
                    downcast::<Time64MicrosecondArray>(array)?.value(row),
                    TimeUnit::Microsecond,
                ),
            )?,
            TimeUnit::Nanosecond => write_json_str(
                output,
                &format_time(
                    downcast::<Time64NanosecondArray>(array)?.value(row),
                    TimeUnit::Nanosecond,
                ),
            )?,
            _ => return unsupported_type(data_type),
        },
        DataType::Timestamp(unit, timezone) => write_json_str(
            output,
            &format_timestamp(array, *unit, timezone.is_some(), row)?,
        )?,
        DataType::List(child) => write_list(
            downcast::<ListArray>(array)?.value(row).as_ref(),
            child,
            output,
        )?,
        DataType::LargeList(child) => write_list(
            downcast::<LargeListArray>(array)?.value(row).as_ref(),
            child,
            output,
        )?,
        DataType::ListView(child) => write_list(
            downcast::<ListViewArray>(array)?.value(row).as_ref(),
            child,
            output,
        )?,
        DataType::LargeListView(child) => write_list(
            downcast::<LargeListViewArray>(array)?.value(row).as_ref(),
            child,
            output,
        )?,
        DataType::FixedSizeList(child, _) => write_list(
            downcast::<FixedSizeListArray>(array)?.value(row).as_ref(),
            child,
            output,
        )?,
        DataType::Struct(fields) => write_struct(
            downcast::<StructArray>(array)?,
            fields.iter().map(AsRef::as_ref),
            row,
            output,
        )?,
        DataType::Map(_, _) => write_map(downcast::<MapArray>(array)?, row, output)?,
        DataType::Dictionary(key_type, value_type) => {
            write_dictionary(array, key_type, value_type, row, output)?
        }
        _ => return unsupported_type(data_type),
    }

    Ok(())
}

fn write_bool(output: &mut impl Write, value: bool) -> Result<()> {
    output.write_all(if value { b"true" } else { b"false" })?;
    Ok(())
}

fn write_float(output: &mut impl Write, value: f64) -> Result<()> {
    if value.is_nan() {
        write_json_str(output, "NaN")
    } else if value == f64::INFINITY {
        write_json_str(output, "+Inf")
    } else if value == f64::NEG_INFINITY {
        write_json_str(output, "-Inf")
    } else {
        serde_json::to_writer(output, &value)?;
        Ok(())
    }
}

fn write_binary(output: &mut impl Write, value: &[u8]) -> Result<()> {
    let mut encoded = String::with_capacity("base64:".len() + value.len().div_ceil(3) * 4);
    encoded.push_str("base64:");
    BASE64_STANDARD.encode_string(value, &mut encoded);
    write_json_str(output, &encoded)
}

fn write_list(array: &dyn Array, child: &Field, output: &mut impl Write) -> Result<()> {
    output.write_all(b"[")?;
    for row in 0..array.len() {
        if row > 0 {
            output.write_all(b",")?;
        }
        write_value(array, child.data_type(), row, output)?;
    }
    output.write_all(b"]")?;
    Ok(())
}

fn write_struct<'a>(
    array: &StructArray,
    fields: impl Iterator<Item = &'a Field>,
    row: usize,
    output: &mut impl Write,
) -> Result<()> {
    output.write_all(b"{")?;
    for (index, field) in fields.enumerate() {
        if index > 0 {
            output.write_all(b",")?;
        }
        write_json_str(output, field.name())?;
        output.write_all(b":")?;
        write_value(array.column(index).as_ref(), field.data_type(), row, output)?;
    }
    output.write_all(b"}")?;
    Ok(())
}

fn write_map(array: &MapArray, row: usize, output: &mut impl Write) -> Result<()> {
    let entries = array.value(row);
    let keys = entries.column(0);
    let values = entries.column(1);
    let (key_field, value_field) = array.entries_fields();

    if map_has_unique_string_keys(keys.as_ref()) {
        output.write_all(b"{")?;
        for index in 0..entries.len() {
            if index > 0 {
                output.write_all(b",")?;
            }
            write_json_str(output, string_value(keys.as_ref(), index)?)?;
            output.write_all(b":")?;
            write_value(values.as_ref(), value_field.data_type(), index, output)?;
        }
        output.write_all(b"}")?;
    } else {
        output.write_all(b"[")?;
        for index in 0..entries.len() {
            if index > 0 {
                output.write_all(b",")?;
            }
            output.write_all(br#"{"key":"#)?;
            write_value(keys.as_ref(), key_field.data_type(), index, output)?;
            output.write_all(br#","value":"#)?;
            write_value(values.as_ref(), value_field.data_type(), index, output)?;
            output.write_all(b"}")?;
        }
        output.write_all(b"]")?;
    }

    Ok(())
}

fn map_has_unique_string_keys(keys: &dyn Array) -> bool {
    let mut seen = HashSet::with_capacity(keys.len());

    for row in 0..keys.len() {
        if keys.is_null(row) {
            return false;
        }
        let Ok(value) = string_value(keys, row) else {
            return false;
        };
        if !seen.insert(value.to_string()) {
            return false;
        }
    }

    true
}

fn string_value(array: &dyn Array, row: usize) -> Result<&str> {
    match array.data_type() {
        DataType::Utf8 => Ok(downcast::<StringArray>(array)?.value(row)),
        DataType::LargeUtf8 => Ok(downcast::<LargeStringArray>(array)?.value(row)),
        DataType::Utf8View => Ok(downcast::<StringViewArray>(array)?.value(row)),
        other => Err(Error::Unsupported(format!(
            "map key type `{other}` cannot be rendered as a JSON object key"
        ))),
    }
}

fn write_dictionary(
    array: &dyn Array,
    key_type: &DataType,
    value_type: &DataType,
    row: usize,
    output: &mut impl Write,
) -> Result<()> {
    match key_type {
        DataType::Int8 => write_dictionary_value::<Int8Type>(array, value_type, row, output),
        DataType::Int16 => write_dictionary_value::<Int16Type>(array, value_type, row, output),
        DataType::Int32 => write_dictionary_value::<Int32Type>(array, value_type, row, output),
        DataType::Int64 => write_dictionary_value::<Int64Type>(array, value_type, row, output),
        DataType::UInt8 => write_dictionary_value::<UInt8Type>(array, value_type, row, output),
        DataType::UInt16 => write_dictionary_value::<UInt16Type>(array, value_type, row, output),
        DataType::UInt32 => write_dictionary_value::<UInt32Type>(array, value_type, row, output),
        DataType::UInt64 => write_dictionary_value::<UInt64Type>(array, value_type, row, output),
        _ => unsupported_type(key_type),
    }
}

fn write_dictionary_value<K: ArrowDictionaryKeyType>(
    array: &dyn Array,
    value_type: &DataType,
    row: usize,
    output: &mut impl Write,
) -> Result<()> {
    let dictionary = downcast::<DictionaryArray<K>>(array)?;
    let Some(value_index) = dictionary.key(row) else {
        output.write_all(b"null")?;
        return Ok(());
    };

    write_value(
        dictionary.values().as_ref(),
        value_type,
        value_index,
        output,
    )
}

fn format_date32(days: i32) -> Result<String> {
    let seconds = i64::from(days) * 86_400;
    let dt = DateTime::<Utc>::from_timestamp(seconds, 0)
        .ok_or_else(|| Error::Unsupported(format!("date value `{days}` is out of range")))?;
    Ok(dt.format("%Y-%m-%d").to_string())
}

fn format_date64(milliseconds: i64) -> Result<String> {
    let seconds = milliseconds.div_euclid(1_000);
    let nanos = milliseconds.rem_euclid(1_000) as u32 * 1_000_000;
    let dt = DateTime::<Utc>::from_timestamp(seconds, nanos).ok_or_else(|| {
        Error::Unsupported(format!("date64 value `{milliseconds}` is out of range"))
    })?;
    Ok(dt.format("%Y-%m-%d").to_string())
}

fn format_time(value: i64, unit: TimeUnit) -> String {
    let nanos = match unit {
        TimeUnit::Second => value * 1_000_000_000,
        TimeUnit::Millisecond => value * 1_000_000,
        TimeUnit::Microsecond => value * 1_000,
        TimeUnit::Nanosecond => value,
    };
    let hours = nanos / 3_600_000_000_000;
    let minutes = (nanos % 3_600_000_000_000) / 60_000_000_000;
    let seconds = (nanos % 60_000_000_000) / 1_000_000_000;
    let fraction = nanos % 1_000_000_000;

    match unit {
        TimeUnit::Second => format!("{hours:02}:{minutes:02}:{seconds:02}"),
        TimeUnit::Millisecond => {
            format!(
                "{hours:02}:{minutes:02}:{seconds:02}.{:03}",
                fraction / 1_000_000
            )
        }
        TimeUnit::Microsecond => {
            format!(
                "{hours:02}:{minutes:02}:{seconds:02}.{:06}",
                fraction / 1_000
            )
        }
        TimeUnit::Nanosecond => format!("{hours:02}:{minutes:02}:{seconds:02}.{fraction:09}"),
    }
}

fn format_timestamp(array: &dyn Array, unit: TimeUnit, utc: bool, row: usize) -> Result<String> {
    let value = match unit {
        TimeUnit::Second => downcast::<TimestampSecondArray>(array)?.value(row),
        TimeUnit::Millisecond => downcast::<TimestampMillisecondArray>(array)?.value(row),
        TimeUnit::Microsecond => downcast::<TimestampMicrosecondArray>(array)?.value(row),
        TimeUnit::Nanosecond => downcast::<TimestampNanosecondArray>(array)?.value(row),
    };

    let (seconds, nanos, precision) = match unit {
        TimeUnit::Second => (value, 0, 0),
        TimeUnit::Millisecond => (
            value.div_euclid(1_000),
            value.rem_euclid(1_000) as u32 * 1_000_000,
            3,
        ),
        TimeUnit::Microsecond => (
            value.div_euclid(1_000_000),
            value.rem_euclid(1_000_000) as u32 * 1_000,
            6,
        ),
        TimeUnit::Nanosecond => (
            value.div_euclid(1_000_000_000),
            value.rem_euclid(1_000_000_000) as u32,
            9,
        ),
    };

    let dt = DateTime::<Utc>::from_timestamp(seconds, nanos)
        .ok_or_else(|| Error::Unsupported(format!("timestamp value `{value}` is out of range")))?;
    let base = dt.format("%Y-%m-%dT%H:%M:%S").to_string();
    let suffix = if utc { "Z" } else { "" };

    if precision == 0 {
        Ok(format!("{base}{suffix}"))
    } else {
        let scaled = match precision {
            3 => nanos / 1_000_000,
            6 => nanos / 1_000,
            _ => nanos,
        };
        Ok(format!("{base}.{scaled:0precision$}{suffix}"))
    }
}

fn write_json_str(output: &mut impl Write, value: &str) -> Result<()> {
    serde_json::to_writer(output, value)?;
    Ok(())
}

fn downcast<T: 'static>(array: &dyn Array) -> Result<&T> {
    array.as_any().downcast_ref::<T>().ok_or_else(|| {
        Error::Unsupported(format!(
            "unsupported Arrow array for data type `{}`",
            array.data_type()
        ))
    })
}

fn unsupported_type<T>(data_type: &DataType) -> Result<T> {
    Err(Error::Unsupported(format!(
        "unsupported Parquet feature for v1: Arrow data type `{data_type}`"
    )))
}
