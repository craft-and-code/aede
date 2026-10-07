# Integer source acceptance fixtures

These 24 native FLAC sources cover 16/24-bit mono/stereo PCM at 44.1, 48, 88.2, 96, 176.4 and 192 kHz. They are software fixtures, not evidence that any hardware route supports those formats or preserves digital output.

Each file contains 4233 complete frames, crossing the 4096-frame decoder boundary. The independent source programme includes leading and intermittent silence, signed endpoints, isolated impulses, one-bit and byte-boundary patterns, and distinguishable left/right channels. The sibling CLI acceptance tests construct ordinary PCM WAV files from the same known source programme instead of deriving their oracle from Aède's decoder.

Run `python3 crates/aede-cli/tests/fixtures/bit-perfect/generate.py` from the repository root with reference `flac 1.5.0` installed to rebuild the sources. The generator writes signed little-endian raw PCM, encodes it with reference libFLAC, decodes every generated file again with reference libFLAC and requires complete byte identity. `manifest.json` records the source PCM MD5/SHA-256 and encoded file SHA-256 without machine paths or timestamps. No external encoder or decoder is needed when running the Rust acceptance tests.

The generated STREAMINFO MD5 is present and remains checked by Aède at complete EOF. Test-induced corruption changes only a copied source's digest and never modifies these fixtures.
