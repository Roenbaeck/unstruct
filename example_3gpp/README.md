# 3GPP measurement examples

`measurements.parser` handles both XML examples and writes one row per counter
value. The dictionary is scoped to each `measInfo` element, so repeated counter
numbers can have different meanings in different blocks.

- `measurements.xml` is a small synthetic example with three measurements. It
  includes counter declarations after their use and reused counter numbers.
- `sample-node.xml` is a sanitized full-size sample with 33 measurement blocks,
  144 measurement objects, and 7,422 measurement values. Its node, cell, network,
  equipment, interface, and job identifiers have been replaced consistently.
  Vendor and software metadata are fictional, and collection timestamps have
  been shifted to an example date while preserving the 15-minute interval.

The full-size sample retains the original counter names, dictionary keys,
measurement values (including comma-separated arrays), XML structure, and
standard namespace information. It includes no mapping back to the original
identifiers. The unused stylesheet reference was removed.

From the repository root:

```console
target/release/unstruct \
  --filename 'example_3gpp/sample-node.xml' \
  --parser example_3gpp/measurements.parser \
  --format parquet \
  --metadata \
  --outfile sample.parquet
```

Using `'example_3gpp/*.xml'` processes both examples into one file with 7,425 rows.
Omit `--format parquet` and use a `.tsv` output filename for TSV output.
