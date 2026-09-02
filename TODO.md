# TODO

This backlog was refreshed after a static code review on 2026-09-02. The review
machine had no Rust toolchain, so every code change below still needs compilation
and executable tests on a Rust-enabled machine.

Priority meanings:

- **P0**: do first; security or work needed to make later changes safe.
- **P1**: possible silent data loss or corrupt output.
- **P2**: robustness and maintainability.
- **P3**: measured performance and product improvements.

## P0 - Dependency and regression baseline

- [ ] Remove transitive `atty 0.2.14` and close Dependabot alert #1
	(`GHSA-g98v-hv3f-hcfr`, `RUSTSEC-2021-0145`). It is pulled in by the locked
	`clap 4.0.26`; `clap 4.0.27` replaced it with `is-terminal`. The manifest's
	`clap = "4.0"` range already permits the fix, so refresh `Cargo.lock` to a
	supported Clap 4 release after deciding and documenting the minimum supported
	Rust version. Confirm with `cargo tree -i atty` and `cargo audit` (or
	`cargo deny check advisories`). The advisory is low severity, Windows-only,
	and normally requires a custom global allocator, but `atty` has no patched
	release and is unmaintained.
- [ ] Turn all four example directories into golden-file integration tests.
	Run each parser against its XML fixture(s), compare exact output with
	`result.txt`, and cover both metadata modes. Preserve these tests before
	changing traversal behavior.
- [ ] Add focused regression fixtures for filters, attributes, namespaces,
	multiple nested records, missing values, malformed XML/config, empty globs,
	and XML values containing tabs/newlines.
- [ ] Add CI checks for `cargo fmt --check`, `cargo clippy -- -D warnings`, tests,
	and dependency advisories.

## P1 - Correctness and data integrity

- [ ] Decide and implement empty-record semantics. The two self-closing
	`<ChangeOfCharCondition/>` elements in
	`example_cdr/sgw1_20220506_0005.xml` currently produce no rows because there
	are no child nodes on which the nested directives can match. Decide whether
	each empty recording element should emit a row with blank child columns, make
	the behavior explicit (and configurable only if both modes are useful), then
	add the expected rows to the CDR golden test. This item from the old TODO is
	still outstanding.
- [ ] Make matcher identity context-aware. Configuration is currently compiled
	into keys containing only `xml-name + depth`; two directives, filters, or
	recording elements with the same name at the same depth but under different
	parents overwrite each other in `HashMap`s. Compile full selector paths (or a
	tree) so separate branches cannot silently capture each other's values.
- [ ] Require the parser configuration grammar to consume the whole file. The
	Pest `config` rule has no `SOI`/`EOI`, so a valid prefix can succeed while
	trailing malformed or unsupported text is ignored; an invalid file may also
	parse as an empty configuration because the top-level expression allows zero
	items. Return a diagnostic with source location instead.
- [ ] Define safe TSV serialization. XML text and attributes are written
	verbatim, so a tab, carriage return, or newline changes the number of columns
	or rows. Either document and implement an escaping convention accepted by the
	target bulk loaders, or use a configurable delimiter/quoting policy through a
	mature CSV/TSV writer. Test Unicode and empty fields too.
- [ ] Reject an output path that is also an input path or is matched by the input
	glob. The output file is created and truncated before input files are read,
	which can destroy an input and then abort parsing it. Prefer writing a
	temporary file and atomically replacing the destination after success.
- [ ] Validate configurations before processing any files. At minimum reject
	duplicate output column names, duplicate/ambiguous selectors, unsupported
	nesting, no directives, and the reserved `_path` column when `--metadata` is
	used. Do not silently overwrite entries in `matcher`, `filters`, or
	`elements`.

## P2 - Reliability and maintainability

- [ ] Refactor `config::parse`, traversal, and the CLI entry point to return
	`Result` instead of calling `process::exit`, `unwrap`, or `expect`. Print
	actionable errors to stderr and return a nonzero exit status for invalid
	config/glob, an unwritable output, read failures, malformed XML, and write or
	flush failures.
- [ ] Choose and document batch error behavior: fail fast without publishing a
	partial output, or continue past bad files and report a final failure summary.
	Include the source filename in XML diagnostics. `--quiet` should suppress
	informational output, not errors.
- [ ] Collect glob matches once. The current two glob walks can see different
	file sets if the directory changes between passes and duplicate filesystem
	work. Sort the collected paths for deterministic output, and warn or fail when
	no files match.
- [ ] Compute metadata paths with `Path`/`PathBuf` components and `strip_prefix`
	rather than string `starts_with` and `replace`. String prefixes confuse sibling
	directories such as `foo` and `foobar`, and `replace` can remove occurrences
	beyond the leading path. Define whether `_path` is relative to the common
	parent, the glob root, or the current directory.
- [ ] Make namespace matching URI-based and scoped. Only named namespace
	declarations visible on the document root are currently collected;
	descendant declarations are missed, multiple prefixes for one URI are
	ambiguous, and XML prefixes are aliases that may change between equivalent
	files. Add namespace declarations to the parser configuration or otherwise
	compile stable `{namespace-uri}local-name` selectors.
- [ ] Split the core transform from CLI/file I/O. Expose a function that accepts
	parsed configuration plus XML and writes to an arbitrary `Write`; keep glob
	expansion, path metadata, and diagnostics in the binary. This will make unit
	and integration testing much simpler.
- [ ] Replace the tuple returned by `config::parse` with named configuration
	types. Model selectors, filters, recording scopes, headers, and level counts
	explicitly so invalid states are harder to represent.
- [ ] Define XML text semantics. `Node::text()` returns only a direct text child;
	mixed-content elements, nested text, CDATA, and formatting whitespace need a
	documented policy and tests.
- [ ] Modernize `.github/workflows/rust.yml`. Upgrade obsolete `checkout`,
	`cache`, `upload-artifact`, and `download-artifact` action versions. Artifact
	v4 also requires unique names for matrix jobs. Pin a Rust toolchain/MSRV, use
	dependency-aware cache keys, and verify whether every advertised GNU/musl
	cross target still builds on current hosted runners.
- [ ] Add package metadata to `Cargo.toml` (`description`, `repository`,
	`license`, `readme`, and `rust-version`) and document the `Cargo.lock` update
	policy for this binary.
- [ ] Refresh the README: correct spelling, link every example, document filters
	and namespaces, explain row creation and empty/missing values, describe exit
	behavior, and replace the old `data`-folder reference with the current
	`example_*` layout.

## P3 - Performance and tool improvements

- [ ] Wrap the output file in `BufWriter`. Rows are currently written directly
	to `File` one field/delimiter at a time; buffering should reduce system calls.
	Propagate the final flush error.
- [ ] Remove `Rc<Node>` allocations. `roxmltree::Node` is a small copyable handle,
	so allocating and reference-counting every traversed node should be
	unnecessary. Confirm against the selected `roxmltree` release.
- [ ] Reduce hot-path allocation. Qualified selector strings are repeatedly
	built with `format!` for each element and attribute. A compiled selector tree,
	borrowed names, or structured keys should remove most of this work while also
	fixing context collisions.
- [ ] Benchmark before and after performance changes using representative small
	CDR batches and large XML files. Track throughput, peak memory, and output
	size; update the README's old 10,000-files/second claim with reproducible
	hardware, dataset, command, and version details.
- [ ] Evaluate streaming XML parsing only if benchmarks show whole-file DOM
	parsing is a memory or latency bottleneck. Keep `roxmltree` for small files if
	its simplicity wins.
- [ ] After deterministic single-threaded behavior is covered, evaluate parallel
	file parsing with ordered output. Bound memory and compare against the cost of
	serial output merging before adding concurrency.
- [ ] Consider quality-of-life options after correctness is stable: stdin/stdout
	support (`-`), configurable delimiter/line ending, a `--check-config` mode,
	explicit relative-path base, and a final files/rows/errors summary.

## Completed or retired from the old TODO

- [x] `--metadata` / `-m` is implemented and appends a `_path` column containing
	the parsed filename. Remaining path correctness and collision checks are
	tracked above.
