use crate::config::Configuration;
use crate::transform::{extract_rows, Row};
use csv::{Terminator, Writer, WriterBuilder};
use std::collections::HashMap;
use std::io::Write;

pub struct TsvWriter<W: Write> {
    output: Writer<W>,
    header: Vec<String>,
}

impl<W: Write> TsvWriter<W> {
    pub fn new(
        output: W,
        configuration: &Configuration,
        include_metadata: bool,
    ) -> Result<Self, String> {
        if include_metadata && configuration.header.iter().any(|column| column == "_path") {
            return Err("Output column _path is reserved when --metadata is enabled".to_owned());
        }

        let mut header = configuration.header.clone();
        if include_metadata {
            header.push("_path".to_owned());
        }
        let mut output = WriterBuilder::new()
            .delimiter(b'\t')
            .terminator(Terminator::Any(b'\n'))
            .from_writer(output);
        output
            .write_record(&header)
            .map_err(|error| format!("Cannot write to output: {error}"))?;

        Ok(Self { output, header })
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
                if column == "_path" {
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
