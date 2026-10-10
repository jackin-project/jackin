<!--
Write one paragraph per section. Describe shipped behavior in plain terms; do
not list files or narrate implementation details. Drop sections that do not
add useful information. In Verify locally, select only gates relevant to the
diff, replace the example test filter with a focused test for the changed code,
and state expected output when exit status alone is unclear.
-->

## Summary

<Briefly say what this pull request changes and who benefits.>

## What ships

- <User-visible or contributor-visible outcome>
- <Relevant documentation or regression coverage outcome>

## Behavior changes

- <Changed behavior, validation, error, or runtime consequence>

## Verify locally

### Static checks

```sh
mbx +1.97.1 fmt --all -- --check
mbx +1.97.1 clippy --workspace --all-targets -- -D warnings
```

### Tests

<Replace the example filter with the smallest test scope for this diff. Keep the full run when relevant.>

```sh
mbx +1.97.1 nextest run -E 'test(/module::tests/)'
mbx +1.97.1 nextest run
```
