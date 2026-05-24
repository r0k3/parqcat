use std::fs::File;
use std::io::Write;

use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

use crate::cli::{Mode, OutputFormat};
use crate::error::Result;

const BATCH_SIZE: usize = 8192;

pub fn run(
    file: File,
    mode: Mode,
    output_format: OutputFormat,
    output: &mut impl Write,
) -> Result<()> {
    match mode {
        Mode::Schema => {
            let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
            crate::render::write_schema(builder.schema(), output)
        }
        Mode::Cat => {
            let Some((builder, _)) = row_reader(ParquetRecordBatchReaderBuilder::try_new(file)?)
            else {
                return Ok(());
            };
            stream(builder, output_format, output)
        }
        Mode::Head { lines } => {
            if lines == 0 {
                return Ok(());
            }
            let Some((builder, _)) = row_reader(ParquetRecordBatchReaderBuilder::try_new(file)?)
            else {
                return Ok(());
            };
            stream(builder.with_limit(lines), output_format, output)
        }
        Mode::Tail { lines } => {
            if lines == 0 {
                return Ok(());
            }
            let Some((builder, total_rows)) =
                row_reader(ParquetRecordBatchReaderBuilder::try_new(file)?)
            else {
                return Ok(());
            };
            let plan = tail_plan(&builder, lines.min(total_rows));
            stream(
                builder
                    .with_row_groups(plan.row_groups)
                    .with_offset(plan.offset)
                    .with_limit(lines),
                output_format,
                output,
            )
        }
    }
}

fn row_reader(
    builder: ParquetRecordBatchReaderBuilder<File>,
) -> Option<(ParquetRecordBatchReaderBuilder<File>, usize)> {
    let total_rows = usize::try_from(builder.metadata().file_metadata().num_rows()).unwrap_or(0);

    if total_rows == 0 {
        return None;
    }

    Some((builder.with_batch_size(BATCH_SIZE), total_rows))
}

fn stream(
    builder: ParquetRecordBatchReaderBuilder<File>,
    output_format: OutputFormat,
    output: &mut impl Write,
) -> Result<()> {
    let reader = builder.build()?;
    let mut table = None;

    for batch in reader {
        let batch = batch?;
        match output_format {
            OutputFormat::Jsonl => crate::render::write_json_batch(&batch, output)?,
            OutputFormat::Table => {
                let table = table.get_or_insert_with(|| crate::render::TableWriter::new(&batch));
                table.write_batch(&batch, output)?;
            }
        }
    }
    Ok(())
}

struct TailPlan {
    row_groups: Vec<usize>,
    offset: usize,
}

fn tail_plan(builder: &ParquetRecordBatchReaderBuilder<File>, wanted_rows: usize) -> TailPlan {
    let metadata = builder.metadata();
    let mut selected_rows = 0usize;
    let mut first_group = metadata.num_row_groups();

    while first_group > 0 && selected_rows < wanted_rows {
        first_group -= 1;
        let rows = usize::try_from(metadata.row_group(first_group).num_rows()).unwrap_or(0);
        selected_rows += rows;
    }

    let row_groups = (first_group..metadata.num_row_groups()).collect::<Vec<_>>();
    let offset = selected_rows.saturating_sub(wanted_rows);

    TailPlan { row_groups, offset }
}
