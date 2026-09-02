# unstruct

Unstruct converts one or more XML documents into one UTF-8, tab-separated file
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

Files are collected once and processed in sorted path order. A pattern that
matches no files is an error. With `--metadata`, `_path` is relative to the
common parent directory of all matched files. Path operations use filesystem
components, so sibling names such as `foo` and `foobar` remain distinct.

Output uses UTF-8, tab delimiters, and LF record terminators. Fields containing
tabs, carriage returns, newlines, or double quotes are quoted according to CSV
rules; embedded double quotes are doubled. Empty and missing values remain empty
fields.

Batch processing is fail-fast. Output is assembled in a temporary file in the
destination directory and atomically published only after every input succeeds
and the writer flushes. A configuration, glob, read, XML parse, write, or flush
failure returns a nonzero status, reports the relevant path on stderr, and does
not publish partial output. An output path that resolves to an input XML file or
the parser configuration is rejected without modifying that file. `--quiet`
suppresses informational stdout but never suppresses errors.

## Options

| Switch | Description |
|---|---|
| `-f, --filename <pattern>` | Input XML path or glob pattern |
| `-o, --outfile <filename>` | Destination TSV file |
| `-p, --parser <filename>` | Parser configuration (default: `unstruct.parser`) |
| `-m, --metadata` | Append a `_path` column for each input file |
| `-q, --quiet` | Suppress informational output |

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
