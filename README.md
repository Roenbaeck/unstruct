# unstruct

Unstruct converts one or more XML documents into one UTF-8, tab-separated or Parquet file
suitable for bulk loading into a relational database. A parser configuration
selects output columns, recording elements, filters, attributes, and namespaces.

## Build and run

Building requires the current stable Rust toolchain. The project does not
maintain a minimum supported Rust version.

```console
cargo build --locked --release
target/release/unstruct \
  --filename 'example_cdr/*.xml' \
  --parser example_cdr/cdr.parser \
  --outfile result.tsv
```

The repository contains tested [BAG](example_bag), [CDR](example_cdr),
[FTP](example_ftp), and [statistics](example_sts) examples. Each directory has
an XML fixture, parser configuration, and golden output with and without path
metadata.

## Configuration

Braces represent XML depth. A directive maps an XML selector to an output
column, while an element in angle brackets starts a recording scope:

```text
{
    <sGW-GPRS-Ascii> {
        subscriber = "servedIMSI"
        duration = "duration"
        durationUnit = "duration/@unit"
        {
            <ChangeOfCharCondition> {
                changedAt = "changeTime"
            }
        }
    }
}
```

Each matched recording element creates a row. Repeated nested recordings create
multiple rows and inherit values from their outer recording. Outer values are
broadcast regardless of sibling order. A matched empty recording still creates
a row; missing elements and attributes produce empty fields.

Selectors can include an ancestor path when local names at the same depth are
ambiguous, for example `left/value` and `right/value`. A filter keeps a recording
only when its selector has the expected value:

```text
["download/@success" = "true"]
```

### Namespaces

Declare configuration aliases before the parser rules and use them in element,
attribute, recording, and filter selectors:

```text
namespace records = "urn:example:records"

{
    <records:Record> {
        identifier = "records:id"
    }
}
```

Aliases are resolved to namespace URIs when the configuration is parsed. They
do not need to match prefixes in the XML document, and declarations may be
scoped below the document root. Unqualified configuration names match only XML
names without a namespace. As required by XML Namespaces, an unprefixed
attribute is not in the element's default namespace.

### Text values

An element value is the concatenation of its direct text and CDATA children in
document order. Text inside nested elements is excluded. Leading, trailing, and
formatting whitespace is preserved. Use a selector for the nested element when
its text should be a separate value.

Configurations are validated before XML processing. Empty configurations,
undeclared or duplicate namespace aliases, duplicate columns, ambiguous
same-scope selectors and filters, duplicate recording selectors, directives
outside a recording scope, and configurations without output columns are
rejected. The `_path` column is reserved when `--metadata` is enabled.

## Output and batches

### Scoped dictionaries

Dictionary declarations resolve identifiers whose meanings are defined elsewhere
in the XML. A dictionary belongs to each instance of its enclosing recording
element. It is available to that recording and its nested recordings, regardless
of declaration order in either the configuration or the XML.

For example, 3GPP measurement files map `measType/@p` to a counter name, and
`r/@p` refers to that name within the same `measInfo` element:

```text
namespace m = "http://www.3gpp.org/ftp/specs/archive/32_series/32.435#measCollec"

{
    {
        <m:measInfo>
        dictionary counters {
            entry = "m:measType"
            key = "@p"
            value = "."
        }
        {
            <m:measValue>
            object = "m:measValue/@measObjLdn"
            {
                <m:r>
                counter = counters["m:r/@p"]
                value = "m:r"
            }
        }
    }
}
```

`entry` is a child element path relative to the recording element, independent
of the declaration's brace depth. `key` and `value` are relative to each entry:
`.` selects direct text, `@name` selects an attribute, and `child/name` or
`child/@name` select through child paths. Namespace aliases apply to all these
selectors. These are child paths, not XPath expressions.

The quoted expression in `counters["m:r/@p"]` is evaluated as an ordinary XML
selector at the directive's depth; its result is the dictionary key. Keys and
values remain exact strings, including whitespace and leading zeros.

Each entry must resolve to exactly one key and one value. Duplicate keys,
missing entry fields, missing lookup keys, and undefined dictionaries are
errors. Duplicate dictionary names in the same recording are rejected. A
nested declaration with the same name shadows the outer dictionary entirely;
an absent key does not fall back to the outer dictionary. Dictionaries never
carry over between sibling recording instances or input files. Errors report
the dictionary, recording location, and input path through the CLI.

See [the complete 3GPP parser](example_3gpp/measurements.parser) for node, job,
period, object, counter, and value columns. It produces one row per `r` element.
The [3GPP examples](example_3gpp) include a small synthetic fixture and a sanitized
full-size sample with 7,422 measurement values.

### Parquet output

```console
target/release/unstruct \
  --filename 'incoming/*.xml' \
  --parser example_3gpp/measurements.parser \
  --format parquet \
  --compression zstd \
  --metadata \
  --outfile measurements.parquet
```

TSV remains the default; the filename extension does not select the format.
Parquet uses Zstandard compression by default; `--compression none` disables
compression. `--compression` is only valid with `--format parquet`.

Every Parquet column, including `_path`, is a non-null UTF-8 string. Missing
ordinary values are empty strings, as in TSV. Numeric-looking values, timestamps,
and comma-separated arrays are preserved as text without type inference.
Columns follow parser declaration order, with `_path` appended when requested.

The writer batches up to 8,192 rows or approximately 8 MiB of string data and
targets row groups of at most 65,536 rows or approximately 64 MiB. It also flushes
row groups when the writer's memory estimate reaches 64 MiB. These are flushing
thresholds, not a hard process memory limit: an individual value can exceed a
threshold, and the XML tree and extracted rows for one input file remain in
memory. The entire batch of files is never loaded at once. A zero-row result
still produces a valid Parquet file with its schema.

### Batch behavior

Files are collected once and processed in sorted path order. A pattern that
matches no files is an error. With `--metadata`, `_path` is relative to the
common parent directory of all matched files. Path operations use filesystem
components, so sibling names such as `foo` and `foobar` remain distinct.

TSV output uses UTF-8, tab delimiters, and LF record terminators. Fields containing
tabs, carriage returns, newlines, or double quotes are quoted according to CSV
rules; embedded double quotes are doubled. Empty and missing values remain empty
fields.

Batch processing is fail-fast. Output is assembled in a temporary file in the
destination directory and atomically published only after every input succeeds
and the writer finishes (including the Parquet footer). A configuration, glob,
read, XML parse, dictionary lookup, write, or flush
failure returns a nonzero status, reports the relevant path on stderr, and does
not publish partial output. An output path that resolves to an input XML file or
the parser configuration is rejected without modifying that file. `--quiet`
suppresses informational stdout but never suppresses errors.

## Options

| Switch | Description |
|---|---|
| `-f, --filename <pattern>` | Input XML path or glob pattern |
| `-o, --outfile <filename>` | Destination TSV or Parquet file |
| `-p, --parser <filename>` | Parser configuration (default: `unstruct.parser`) |
| `-m, --metadata` | Append a `_path` column for each input file |
| `-q, --quiet` | Suppress informational output |
| `--format <tsv\|parquet>` | Output format (default: `tsv`) |
| `--compression <zstd\|none>` | Parquet compression (default: `zstd` for Parquet) |

Run `unstruct --help` for the generated command-line help.

## Development policy

`Cargo.lock` is committed because unstruct is an application. Dependency
updates are intentional: run `cargo update`, review the lockfile diff, and pass
formatting, compilation, tests, Clippy, and `cargo audit` before committing it.
CI follows current stable Rust and uses recent stable direct dependencies.

Pushes and pull requests run the quality suite. Tags and the manual
`build_release_artifacts` workflow create archives for x86-64 Linux GNU, x86-64
Linux musl, x86-64 macOS, and x86-64 Windows. The separate manual inputs can
regenerate `Cargo.lock` or create a Windows development binary.
