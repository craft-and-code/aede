# CLI output references

The snapshot tests in `../end_to_end.rs` run the real CLI on copies of the checked-in audio fixtures and compare its complete standard output with these files. They disable terminal colors and exclude the scan progress output, which contains elapsed time and a temporary path. The track page replaces the copied music folder path with `<MUSIC>`. The fingerprint case uses a single tagged file so it needs no external fingerprinting program.

Run the focused check with:

```sh
cargo test -p aede-cli --test end_to_end snapshot
```

When a presentation change is intentional, run the test, inspect the old and new output, then update the affected reference file in the same change. Do not regenerate all references without reviewing the differences.
