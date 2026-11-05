use arrow_array::{Array, StringArray};
use arrow_schema::DataType;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::basic::Compression;
use std::fs::{self, File};
use std::path::Path;
use std::process::{Command, Output};
use unstruct::config::parse;
use unstruct::output::{ParquetCompression, ParquetWriter};

fn read_rows(path: &Path) -> (Vec<String>, Vec<Vec<String>>) {
    let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(path).unwrap()).unwrap();
    let header = builder
        .schema()
        .fields()
        .iter()
        .map(|field| {
            assert_eq!(field.data_type(), &DataType::Utf8);
            assert!(!field.is_nullable());
            field.name().clone()
        })
        .collect();
    let mut rows = Vec::new();
    for batch in builder.build().unwrap() {
        let batch = batch.unwrap();
        for index in 0..batch.num_rows() {
            rows.push(
                batch
                    .columns()
                    .iter()
                    .map(|column| {
                        assert_eq!(column.null_count(), 0);
                        column
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .unwrap()
                            .value(index)
                            .to_owned()
                    })
                    .collect(),
            );
        }
    }
    (header, rows)
}

fn cli(directory: &Path, format: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_unstruct"))
        .args(["--filename"])
        .arg(directory.join("*.xml"))
        .arg("--parser")
        .arg(directory.join("config.parser"))
        .arg("--outfile")
        .arg(directory.join(format!("result.{format}")))
        .args(["--format", format, "--metadata", "--quiet"])
        .args(extra)
        .output()
        .unwrap()
}

fn assert_success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn glob_dictionary_output_matches_tsv_and_preserves_string_values() {
    let directory = tempfile::tempdir().unwrap();
    let dir = directory.path();
    fs::write(
        dir.join("config.parser"),
        r#"
        <root>
        dictionary d { entry = "type" key = "@p" value = "." }
        { <r> counter = d["r/@p"] value = "r" missing = "r/@absent" }
    "#,
    )
    .unwrap();
    fs::write(dir.join("a.xml"), "<root><r p=\"1\">001</r><r p=\"1\">0,1,2</r><r p=\"1\"> räksmörgås\t\n&amp;\" </r><r p=\"1\"/><type p=\"1\">first</type></root>").unwrap();
    fs::write(
        dir.join("b.xml"),
        "<root><type p=\"1\">second</type><r p=\"1\">1e300</r></root>",
    )
    .unwrap();
    assert_success(cli(dir, "tsv", &[]));
    assert_success(cli(dir, "parquet", &[]));
    let (header, rows) = read_rows(&dir.join("result.parquet"));
    assert_eq!(header, ["counter", "value", "missing", "_path"]);
    assert_eq!(
        rows,
        [
            vec!["first", "001", "", "a.xml"],
            vec!["first", "0,1,2", "", "a.xml"],
            vec!["first", " räksmörgås\t\n&\" ", "", "a.xml"],
            vec!["first", "", "", "a.xml"],
            vec!["second", "1e300", "", "b.xml"],
        ]
    );
    let mut tsv = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_path(dir.join("result.tsv"))
        .unwrap();
    assert_eq!(tsv.headers().unwrap().iter().collect::<Vec<_>>(), header);
    let tsv_rows: Vec<Vec<String>> = tsv
        .records()
        .map(|row| row.unwrap().iter().map(str::to_owned).collect())
        .collect();
    assert_eq!(rows, tsv_rows);
    let builder =
        ParquetRecordBatchReaderBuilder::try_new(File::open(dir.join("result.parquet")).unwrap())
            .unwrap();
    for group in builder.metadata().row_groups() {
        for column in group.columns() {
            assert!(matches!(column.compression(), Compression::ZSTD(_)));
        }
    }
    assert_success(cli(dir, "parquet", &["--compression", "none"]));
    assert_eq!(read_rows(&dir.join("result.parquet")).1, rows);
    let builder =
        ParquetRecordBatchReaderBuilder::try_new(File::open(dir.join("result.parquet")).unwrap())
            .unwrap();
    assert_eq!(
        builder.metadata().row_group(0).column(0).compression(),
        Compression::UNCOMPRESSED
    );
}

#[test]
fn crosses_batch_and_row_group_boundaries_without_losing_or_reordering_rows() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("many.parquet");
    let config = parse("{ <r> value = \"r\" }").unwrap();
    let mut writer = ParquetWriter::new(
        File::create(&path).unwrap(),
        &config,
        false,
        ParquetCompression::Zstd,
    )
    .unwrap();
    // Each call represents a separate file; the final batch is deliberately partial.
    for file in 0..9 {
        let mut xml = String::from("<root>");
        for index in 0..8001 {
            xml.push_str(&format!("<r>{file}:{index:05}</r>"));
        }
        xml.push_str("</root>");
        assert_eq!(writer.write_xml(&config, &xml, None).unwrap(), 8001);
    }
    writer.finish().unwrap();
    let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(&path).unwrap()).unwrap();
    assert!(builder.metadata().num_row_groups() > 1);
    assert_eq!(builder.metadata().file_metadata().num_rows(), 9 * 8001);
    let (_, rows) = read_rows(&path);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row, &[format!("{}:{:05}", index / 8001, index % 8001)]);
    }
}

#[test]
fn zero_rows_produce_a_readable_file_with_schema() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("empty.parquet");
    let config = parse("{ <r> value = \"r\" }").unwrap();
    let mut writer = ParquetWriter::new(
        File::create(&path).unwrap(),
        &config,
        false,
        ParquetCompression::Zstd,
    )
    .unwrap();
    assert_eq!(writer.write_xml(&config, "<root/>", None).unwrap(), 0);
    writer.finish().unwrap();
    assert_eq!(read_rows(&path), (vec!["value".into()], vec![]));
}

#[test]
fn path_column_is_ordinary_text_when_metadata_is_disabled() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("path.parquet");
    let config = parse(r#"<root> _path = "root""#).unwrap();
    let mut writer = ParquetWriter::new(
        File::create(&path).unwrap(),
        &config,
        false,
        ParquetCompression::Zstd,
    )
    .unwrap();
    writer
        .write_xml(&config, "<root>original</root>", None)
        .unwrap();
    writer.finish().unwrap();
    assert_eq!(
        read_rows(&path),
        (vec!["_path".into()], vec![vec!["original".into()]])
    );
    assert_eq!(
        unstruct::output::write_tsv(&config, "<root>original</root>", Vec::new()).unwrap(),
        b"_path\noriginal\n"
    );
}

#[test]
fn failed_later_file_preserves_existing_output_and_reports_dictionary_and_path() {
    let directory = tempfile::tempdir().unwrap();
    let dir = directory.path();
    fs::write(
        dir.join("config.parser"),
        r#"<root> dictionary d { entry = "type" key = "@p" value = "." } { <r> name = d["r/@p"] }"#,
    )
    .unwrap();
    fs::write(
        dir.join("a.xml"),
        r#"<root><type p="1">ok</type><r p="1"/></root>"#,
    )
    .unwrap();
    fs::write(dir.join("b.xml"), r#"<root><r p="99"/></root>"#).unwrap();
    for format in ["tsv", "parquet"] {
        let path = dir.join(format!("result.{format}"));
        fs::write(&path, "existing output").unwrap();
        let output = cli(dir, format, &[]);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("b.xml") && stderr.contains("Missing key \"99\" in dictionary \"d\""),
            "{stderr}"
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "existing output");
    }
    assert_eq!(
        fs::read_dir(dir).unwrap().count(),
        5,
        "temporary output was not cleaned up"
    );
}

#[test]
fn rejects_reserved_metadata_and_compression_for_tsv() {
    let directory = tempfile::tempdir().unwrap();
    let dir = directory.path();
    fs::write(dir.join("config.parser"), r#"<root> _path = "root""#).unwrap();
    fs::write(dir.join("a.xml"), "<root/>").unwrap();
    let output = cli(dir, "parquet", &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("reserved"));
    assert!(!dir.join("result.parquet").exists());
    let output = cli(dir, "tsv", &["--compression", "zstd"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires --format parquet"));
}

#[test]
fn parquet_io_errors_propagate() {
    struct FailingWriter;
    impl std::io::Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("test write failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let config = parse("<root> value = \"root\"").unwrap();
    let result = ParquetWriter::new(FailingWriter, &config, false, ParquetCompression::Zstd)
        .and_then(|mut writer| {
            writer.write_xml(&config, "<root>data</root>", None)?;
            writer.finish()
        });
    match result {
        Err(error) => assert!(error.contains("test write failure"), "{error}"),
        Ok(_) => panic!("write failure was ignored"),
    }
}
