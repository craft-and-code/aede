# Aède — Editorial background

Aède is a music library engine written in Rust. It turns audio folders into a connected catalogue of albums, artists, tracks, credits and relationships. It runs locally through its command line and can expose an authenticated API for desktop clients.

## Features

Incremental scanning; deterministic catalogue and attributed metadata; explicit external enrichment; Doctor diagnostics; accounts and private data; local HTTP or configured HTTPS server; playback, M3U exports and verified copying; FlacCompagnon analyses; presentation API for Phémios.

## Approach

Source audio files and tags are unchanged. Personal data is separate from the catalogue and account-bound. External enrichment is explicit; missing information is not presented as a negative result.

## Availability

Open source under MPL-2.0. Available releases are published in the official repository.

[Aède](https://craft-and-code.github.io/aede/) · [FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/)
