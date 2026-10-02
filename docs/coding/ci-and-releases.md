# Local CI and GitHub releases

The CI workflow is already versioned in `.github/workflows/ci.yml`. Use Rust 1.89, the declared minimum version, when reproducing its checks; a newer compiler can accept syntax that CI rejects.

## Run the checks locally

With rustup installed, on macOS or Linux:

```sh
rustup toolchain install 1.89 --component rustfmt --component clippy
cargo +1.89 fetch --locked
RUSTUP_TOOLCHAIN=1.89 bash tools/check.sh
```

Linux GNU builds also need `pkg-config` and ALSA development headers (`sudo apt install pkg-config libasound2-dev` on Ubuntu). Allow local socket communication for the integration tests.

On Windows, reproduce the Windows job in PowerShell:

```powershell
rustup toolchain install 1.89
cargo +1.89 fetch --locked
python tools/update_flaccompagnon_tests.py
cargo +1.89 test --locked
```

Windows filesystem behavior requires Windows itself, either a local machine, a VM, or the native GitHub runner. Compiling on macOS does not verify it.

## Inspect GitHub failures

With GitHub CLI authenticated, from the repository:

```sh
gh run list --workflow ci.yml --limit 5
gh run view RUN_ID --log-failed
```

Check the run's commit before comparing it with the local checkout. Re-running an old run still tests its original commit.

For Linux workflow execution in Docker, an optional [act installation](https://nektosact.com/installation/) can run:

```sh
act push -W .github/workflows/ci.yml -j check --matrix os:ubuntu-22.04 -P ubuntu-22.04=catthehacker/ubuntu:act-22.04 --container-architecture linux/amd64
```

This downloads actions and a container image. It approximates the GitHub environment; runner images differ. [act's runner documentation](https://nektosact.com/usage/runners.html) describes host execution for Windows/macOS on those operating systems, rather than emulation on another system.

## Prepare a release

The Release workflow builds macOS Apple Silicon, macOS Intel, Linux x86_64 musl and Windows x64 archives. Tests for every target must pass before a draft release is created. Linux archives require ffplay for playback; GNU source builds support native ALSA output. Windows terminal playback controls and local writer delegation remain unavailable.

Run Release manually from GitHub Actions to build downloadable artifacts without creating a release. This is the rehearsal for packaging on all four targets.

For a release, commit the intended version and matching lockfile, verify CI, then push the matching version tag (currently `v0.3.0`). The tag must match the workspace version. The workflow attaches archives and SHA-256 checksums to a **draft**. Review its notes and downloads before publishing it in GitHub Releases. Builds are unsigned.

Version-specific highlights live in `docs/releases/vVERSION.md`; the workflow uses them when present and adds download instructions and the commit history. New versions without a notes file receive a reminder to write highlights.

## Website and user documentation

The `Site` workflow builds the complete publication into `dist-site/`: the Onde landing page, English counterpart, user manual, CLI reference, server guide, DSP guide and Rustdoc. Pull requests build and validate an artifact without publishing it. Pushes to the default branch and manual runs publish the same validated artifact to `gh-pages`.

User-facing Markdown is the source of truth. English guides live in `docs/manual/`, `docs/cli/`, `docs/server/` and `docs/dsp/`; translated guides live under `docs/fr/`. The `docs/site-*.json` manifests map titles, descriptions, navigation groups and bilingual paths. Existing topic references are published directly from their Markdown, rather than copied into hand-maintained HTML. Rendering requires Python's standard library and no package installation.

```sh
python3 tools/build_site_tests.py
python3 tools/build-site.py --check
python3 tools/preview-site.py
```

The output is ignored by Git. Rustdoc is built separately with the Rust toolchain and copied to `dist-site/docs/rust/`; `tools/check.sh` checks the combined publication. See [site maintenance](../../site/README.md) for preview, translation and form configuration. The signup form uses a native POST to FormSubmit, which requires the recipient to activate their address after the first submission; website builds do not send email or activate the recipient.
