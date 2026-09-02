# TODO

This backlog was refreshed after a static code review on 2026-09-02 and is being
validated with compilation, executable tests, Clippy, and dependency auditing.

Priority meanings:

- **P0**: do first; security or work needed to make later changes safe.
- **P1**: possible silent data loss or corrupt output.
- **P2**: robustness and maintainability.
- **P3**: measured performance and product improvements.

## P0 - Dependency and regression baseline

- [x] Remove transitive `atty 0.2.14` and close Dependabot alert #1
	(`GHSA-g98v-hv3f-hcfr`, `RUSTSEC-2021-0145`). It is pulled in by the locked
	`clap 4.0.26`; `clap 4.0.27` replaced it with `is-terminal`. The manifest's
	Clap requirement and all other direct dependencies have been updated to recent
	stable releases under the project's current-stable Rust policy. Confirmed with
	`cargo tree -i atty` and `cargo audit`. The advisory was low severity,
	Windows-only, and normally required a custom global allocator, but `atty` had
	no patched release and was unmaintained.
- [x] Turn all four example directories into golden-file integration tests.
	Run each parser against its XML fixture(s), compare exact output with
	`result.txt`, and cover both metadata modes. Preserve these tests before
	changing traversal behavior.
- [x] Add focused regression fixtures for filters, attributes, namespaces,
	multiple nested records, missing values, malformed XML/config, empty globs,
	and XML values containing tabs/newlines.
- [x] Push and validate the new CI workflow. It now checks formatting,
	compilation, Clippy, tests, and dependency advisories on a hosted Rust runner;
	confirm the first run and fix any baseline failures it exposes.

## P1 - Correctness and data integrity

- [x] Empty recording elements emit rows with blank child columns. The two
	self-closing `<ChangeOfCharCondition/>` elements in the fifth CDR fixture are
	covered by both CDR golden files.
- [x] Outer-scope context is broadcast to every repeated child row regardless
	of sibling order ([issue #6](https://github.com/Roenbaeck/unstruct/issues/6)).
	The BAG goldens now contain both `<sl:standBestand>` rows.
- [x] Matcher identity is context-aware. Configuration compiles into a recording
	tree, and ancestor-qualified selectors distinguish same-name elements in
	separate branches without collisions.
- [x] The parser configuration grammar consumes the whole file and reports
	malformed or trailing content with source locations.
- [x] TSV serialization uses the `csv` writer with tab delimiters and LF record
	terminators. Structural characters are quoted, Unicode is preserved, and
	empty fields remain empty.
- [x] Output paths are checked against matched XML and parser paths before
	writing. Output is built in a same-directory temporary file and atomically
	published only after successful parsing and flushing.
- [x] Configurations are validated before processing files. Duplicate columns,
	ambiguous same-scope selectors and filters, duplicate recording selectors,
	unsupported scope placement, no directives, and metadata's reserved `_path`
	column are rejected.

## P2 - Reliability and maintainability

- [x] Configuration parsing, transformation, serialization, and CLI orchestration
	propagate `Result`; production paths contain no `unwrap`, `expect`, or
	`process::exit`. Actionable failures are printed to stderr with a nonzero exit
	status.
- [x] Batch processing is documented and tested as fail-fast. A failed input
	does not publish partial output, XML diagnostics include the source filename,
	and `--quiet` suppresses informational output but not errors.
- [x] Glob matches are collected once, sorted for deterministic output, and an
	empty match fails before output creation.
- [x] Metadata paths use `Path` components and `strip_prefix`. `_path` is
	relative to the common parent of all matched inputs, including for sibling
	directories such as `foo` and `foobar`.
- [x] Namespace declarations in parser configurations compile aliases to stable
	namespace URIs. Matching is independent of XML prefixes and supports default
	and descendant-scoped namespace declarations.
- [x] Core transformation and TSV serialization live in the library. `write_tsv`
	accepts parsed configuration, XML text, and an arbitrary `Write`; the binary
	retains glob expansion, metadata paths, atomic publication, and diagnostics.
- [x] Named `Configuration`, `Recording`, `Selector`, `XmlName`, `Column`, and
	`Filter` types model the parsed configuration.
- [x] Element values concatenate direct text and CDATA children in document
	order, exclude nested element text, and preserve whitespace. Unit tests cover
	mixed content, CDATA, and formatting whitespace.
- [ ] The release workflow now uses current action majors and advertises only
	x86-64 GNU, musl, macOS, and Windows targets with valid linker setup. Native
	GNU and musl release builds pass locally; run the manual
	`build_release_artifacts` workflow after pushing to verify all hosted runners.
- [x] `Cargo.toml` includes description, repository, MIT license, and README
	metadata. The committed binary lockfile and its update policy are documented.
- [x] The README links every example and documents configuration, filters,
	namespaces, row and text semantics, metadata paths, output quoting, failures,
	atomic batch behavior, options, dependencies, and CI targets.

## P3 - Performance and tool improvements

- [x] TSV output uses `csv::Writer`, which buffers internally, and propagates its
	final flush error.
- [x] Traversal uses copyable `roxmltree::Node` handles without `Rc` allocation.
- [x] Selectors compile into structured URI/local-name keys; traversal compares
	expanded names without formatting qualified selector strings.
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
