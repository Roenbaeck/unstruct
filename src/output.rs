use crate::config::Configuration;
use crate::transform::{extract_rows, Row};
use arrow_array::{ArrayRef, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use clap::ValueEnum;
use csv::{Terminator, Writer, WriterBuilder};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Tsv,
    Parquet,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum ParquetCompression {
    #[default]
    Zstd,
    None,
}

fn output_header(
    configuration: &Configuration,
    include_metadata: bool,
) -> Result<Vec<String>, String> {
    if include_metadata && configuration.header.iter().any(|column| column == "_path") {
        return Err("Output column _path is reserved when --metadata is enabled".to_owned());
    }
    let mut header = configuration.header.clone();
    if include_metadata {
        header.push("_path".to_owned());
    }
    Ok(header)
}

pub enum OutputWriter<W: Write + Send> {
    Tsv(Box<TsvWriter<W>>),
    Parquet(Box<ParquetWriter<W>>),
}

impl<W: Write + Send> OutputWriter<W> {
    pub fn new(
        output: W,
        configuration: &Configuration,
        include_metadata: bool,
        format: OutputFormat,
        compression: ParquetCompression,
    ) -> Result<Self, String> {
        match format {
            OutputFormat::Tsv => Ok(Self::Tsv(Box::new(TsvWriter::new(
                output,
                configuration,
                include_metadata,
            )?))),
            OutputFormat::Parquet => Ok(Self::Parquet(Box::new(ParquetWriter::new(
                output,
                configuration,
                include_metadata,
                compression,
            )?))),
        }
    }

    pub fn write_xml(
        &mut self,
        configuration: &Configuration,
        xml: &str,
        metadata_path: Option<&str>,
    ) -> Result<usize, String> {
        match self {
            Self::Tsv(writer) => writer.write_xml(configuration, xml, metadata_path),
            Self::Parquet(writer) => writer.write_xml(configuration, xml, metadata_path),
        }
    }

    pub fn finish(self) -> Result<W, String> {
        match self {
            Self::Tsv(writer) => writer.finish(),
            Self::Parquet(writer) => writer.finish(),
        }
    }
}

const BATCH_ROWS: usize = 8192;
const BATCH_BYTES: usize = 8 * 1024 * 1024;
const ROW_GROUP_ROWS: usize = 65_536;
const ROW_GROUP_BYTES: usize = 64 * 1024 * 1024;

pub struct ParquetWriter<W: Write + Send> {
    output: ArrowWriter<W>,
    header: Vec<String>,
    schema: Arc<Schema>,
    columns: Vec<Vec<String>>,
    buffered_rows: usize,
    buffered_bytes: usize,
    include_metadata: bool,
}

impl<W: Write + Send> ParquetWriter<W> {
    pub fn new(
        output: W,
        configuration: &Configuration,
        include_metadata: bool,
        compression: ParquetCompression,
    ) -> Result<Self, String> {
        let header = output_header(configuration, include_metadata)?;
        let schema = Arc::new(Schema::new(
            header
                .iter()
                .map(|name| Field::new(name, DataType::Utf8, false))
                .collect::<Vec<_>>(),
        ));
        let compression = match compression {
            ParquetCompression::Zstd => Compression::ZSTD(ZstdLevel::default()),
            ParquetCompression::None => Compression::UNCOMPRESSED,
        };
        let properties = WriterProperties::builder()
            .set_compression(compression)
            .set_dictionary_enabled(true)
            .set_max_row_group_row_count(Some(ROW_GROUP_ROWS))
            .set_max_row_group_bytes(Some(ROW_GROUP_BYTES))
            .build();
        let output = ArrowWriter::try_new(output, schema.clone(), Some(properties))
            .map_err(|error| format!("Cannot initialize Parquet output: {error}"))?;
        let columns = vec![Vec::new(); header.len()];
        Ok(Self {
            output,
            header,
            schema,
            columns,
            buffered_rows: 0,
            buffered_bytes: 0,
            include_metadata,
        })
    }

    pub fn write_xml(
        &mut self,
        configuration: &Configuration,
        xml: &str,
        metadata_path: Option<&str>,
    ) -> Result<usize, String> {
        let rows = extract_rows(configuration, xml)?;
        let count = rows.len();
        for mut row in rows {
            for (name, column) in self.header.iter().zip(&mut self.columns) {
                let value = if self.include_metadata && name == "_path" {
                    metadata_path.unwrap_or_default().to_owned()
                } else {
                    row.remove(name).unwrap_or_default()
                };
                self.buffered_bytes += value.len();
                column.push(value);
            }
            self.buffered_rows += 1;
            if self.buffered_rows >= BATCH_ROWS || self.buffered_bytes >= BATCH_BYTES {
                self.flush_batch()?;
            }
        }
        Ok(count)
    }

    fn flush_batch(&mut self) -> Result<(), String> {
        if self.buffered_rows == 0 {
            return Ok(());
        }
        let arrays: Vec<ArrayRef> = self
            .columns
            .iter_mut()
            .map(|column| Arc::new(StringArray::from(std::mem::take(column))) as ArrayRef)
            .collect();
        let batch = RecordBatch::try_new(self.schema.clone(), arrays)
            .map_err(|error| format!("Cannot build Parquet batch: {error}"))?;
        self.output
            .write(&batch)
            .map_err(|error| format!("Cannot write Parquet batch: {error}"))?;
        self.buffered_rows = 0;
        self.buffered_bytes = 0;
        if self.output.memory_size() >= ROW_GROUP_BYTES
            || self.output.in_progress_size() >= ROW_GROUP_BYTES
        {
            self.output
                .flush()
                .map_err(|error| format!("Cannot flush Parquet row group: {error}"))?;
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<W, String> {
        self.flush_batch()?;
        self.output
            .into_inner()
            .map_err(|error| format!("Cannot finalize Parquet output: {error}"))
    }
}

pub struct TsvWriter<W: Write> {
    output: Writer<W>,
    header: Vec<String>,
    include_metadata: bool,
}

impl<W: Write> TsvWriter<W> {
    pub fn new(
        output: W,
        configuration: &Configuration,
        include_metadata: bool,
    ) -> Result<Self, String> {
        let header = output_header(configuration, include_metadata)?;
        let mut output = WriterBuilder::new()
            .delimiter(b'\t')
            .terminator(Terminator::Any(b'\n'))
            .from_writer(output);
        output
            .write_record(&header)
            .map_err(|error| format!("Cannot write to output: {error}"))?;

        Ok(Self {
            output,
            header,
            include_metadata,
        })
    }

    pub fn write_xml(
        &mut self,
        configuration: &Configuration,
        xml: &str,
        metadata_path: Option<&str>,
    ) -> Result<usize, String> {
        let rows = extract_rows(configuration, xml)?;
        let row_count = rows.len();
        for row in &rows {
            self.write_row(row, metadata_path)?;
        }
        Ok(row_count)
    }

    fn write_row(&mut self, row: &Row, metadata_path: Option<&str>) -> Result<(), String> {
        let values: HashMap<&str, &str> = row
            .iter()
            .map(|(column, value)| (column.as_str(), value.as_str()))
            .collect();
        self.output
            .write_record(self.header.iter().map(|column| {
                if self.include_metadata && column == "_path" {
                    metadata_path.unwrap_or_default()
                } else {
                    values.get(column.as_str()).copied().unwrap_or_default()
                }
            }))
            .map_err(|error| format!("Cannot write to output: {error}"))
    }

    pub fn finish(self) -> Result<W, String> {
        self.output
            .into_inner()
            .map_err(|error| format!("Cannot flush output: {}", error.error()))
    }
}

pub fn write_tsv<W: Write>(
    configuration: &Configuration,
    xml: &str,
    output: W,
) -> Result<W, String> {
    let mut output = TsvWriter::new(output, configuration, false)?;
    output.write_xml(configuration, xml, None)?;
    output.finish()
}

#[cfg(test)]
mod tests {
    use super::write_tsv;
    use crate::config::parse;

    #[test]
    fn writes_configuration_and_xml_to_arbitrary_writer() {
        let configuration = parse("{ <record> { value = \"value\" } }").unwrap();
        let output = write_tsv(
            &configuration,
            "<root><record><value>test</value></record></root>",
            Vec::new(),
        )
        .unwrap();

        assert_eq!(output, b"value\ntest\n");
    }
}
