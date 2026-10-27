# Local CI and GitHub releases

The CI workflow is already versioned in `.github/workflows/ci.yml`. Use Rust 1.89, the declared minimum version, when reproducing its checks; a newer compiler can accept syntax that CI rejects.

The source branch is `main`. The separate `gh-pages` branch contains only the generated website and must not be merged into the source tree. CI, security checks and site publication follow `main`; documentation source/edit links use that branch too.

## Run the checks locally

With rustup installed, on macOS or Linux:

```sh
rustup toolchain install 1.89 --component rustfmt --component clippy
cargo +1.89 fetch --locked
RUSTUP_TOOLCHAIN=1.89 bash tools/check.sh
```

Linux GNU builds also need `pkg-config` and ALSA development headers (`sudo apt install pkg-config libasound2-dev` on Ubuntu). Allow local socket communication for the integration tests.

The Unix gate also tests masked account entry in real pseudo-terminals: no echo between keys or during confirmation, Unicode editing, cancellation and mode restoration, queued-paste cleanup, rejected flow-control characters, byte limits, JSON output, and concurrent initialization. It needs the standard `stty` utility, also used by local playback controls. Portable password-input rules, Windows key-event/UTF-16 cases, playback key mappings and redirected-input account integration tests run through Cargo on all three platforms. Windows playback uses native console events and shares the Unix transport state machine, but its real keyboard/mode-restoration and output acceptance requires a Windows host.

On Windows, reproduce the Windows job in PowerShell:

```powershell
rustup toolchain install 1.89
cargo +1.89 fetch --locked
python tools/update_flaccompagnon_tests.py
python tools/project_stats_tests.py
python tools/build_site_tests.py
python tools/gapless_tests.py
cargo +1.89 test --locked
```

Windows filesystem behavior requires Windows itself, either a local machine, a VM, or the native GitHub runner. Compiling on macOS does not verify it.

## Windows console playback acceptance

Run the current executable in Windows Terminal or a classic console using PowerShell; PowerShell ISE and redirected input do not supply native console controls. Cargo's portable mapping/transport tests and an unattended CI job cannot replace an interactive keyboard trial. Start with a quiet output level and the intended device selected as the Windows default. Record the binary revision, Rust version, console host, output backend/device and observed behavior, without private paths or account details.

From the repository, prepare disposable synthetic audio and a separate data folder. Python is required for this fixture; WAV preparation needs no FFmpeg. These commands do not play audio until `play`:

```powershell
cargo +1.89 build --release --locked -p aede-cli
$aedeTrial = Join-Path ([System.IO.Path]::GetTempPath()) ("aede-console-" + [guid]::NewGuid().ToString("N"))
$aedeProbe = Join-Path $aedeTrial "probe"
$aedeData = Join-Path $aedeTrial "data"
python tools/gapless.py prepare $aedeProbe --seconds 8
$aedeBinary = ".\target\release\aede.exe"
& $aedeBinary scan $aedeProbe --data $aedeData
& $aedeBinary play (Join-Path $aedeProbe "whole.wav") --normalize off --repeat all --data $aedeData
```

If fixture preparation or scanning fails, stop and resolve that diagnostic before playing. The final command uses the normal native-then-ffplay choice; note which output was selected. Check Space pauses/resumes without Enter or echoed characters. While paused, press `r` to change repeat and verify that pause remains active. Resume, use `]` and `[` to move ten seconds, then press Ctrl-C: playback must stop through the history path and return successfully to PowerShell. Type a normal PowerShell command afterward and verify that text echoes and waits for Enter again. Inspect the trial's listening records with `& $aedeBinary history --data $aedeData`; skipped prefixes and paused time must not be counted, and a visited track with seeking remains incomplete.

Then start the three-part selection:

```powershell
& $aedeBinary play (Join-Path $aedeProbe "split.m3u") --normalize off --repeat all --data $aedeData
```

Check `n`/Right advances once, `p`/Left within the first three seconds returns to the preceding occurrence, and `p` after three seconds restarts the current occurrence. Exercise `r` through off/one/all, including a natural repeat and Next bypassing repeat-one. Exercise `z` through off/random/smart while playing and paused: current audio and pause must be retained. These synthetic files have no genre tags, so smart reports its fallback to uniform order; use a separate tagged selection when accepting genre-based transitions. Verify a natural repeat-all cycle and use `q` or `s` to stop. Repeat with `$env:AEDE_AUDIO_BACKEND = "native"` and, when installed, `"ffplay"`; remove the variable afterward with `Remove-Item Env:AEDE_AUDIO_BACKEND`. Requiring native output must report an unavailable device rather than silently fall back.

Verify restoration after a decoder error as well as normal Stop. Create a deliberately invalid fixture, run it, then type an ordinary PowerShell command and check normal echo/line editing:

```powershell
$aedeBroken = Join-Path $aedeTrial "broken.wav"
[System.IO.File]::WriteAllText($aedeBroken, "deliberately invalid WAV fixture")
& $aedeBinary play $aedeBroken --data $aedeData
```

Finally redirect stdin with a value that would otherwise mean Stop:

```powershell
"q" | & $aedeBinary play (Join-Path $aedeProbe "split.m3u") --normalize off --data $aedeData
```

The full selection must advance automatically and finish; the piped `q` must not become a transport command. Keep the logs/history and any refusal/error details with the trial record. Console restoration and audible control behavior do not establish physical gapless playback; that needs the separate [captured-output procedure](gapless-measurement.md).

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

The Release workflow builds macOS Apple Silicon, Linux x86_64 musl, Linux ARM64 musl and Windows x64 CLI archives. The desktop App matrix remains macOS Apple Silicon, Linux x86_64 and Windows x64. Future releases do not include macOS Intel archives. Tests for every target must pass before a draft release is created. Linux archives require ffplay for playback; GNU source builds support native ALSA output. Windows console playback controls are implemented, with native console/output acceptance still pending; local writer delegation remains Unix-only.

The ARM64 CLI target is `aarch64-unknown-linux-musl`, labelled `Linux-ARM64`, on GitHub’s native `ubuntu-22.04-arm` runner. The workflow uses Rust 1.89, the locked dependency graph and the native musl C compiler for both C dependencies and final linking. Linux jobs require FFmpeg conversion regressions, execute Cargo tests on the target architecture, and reject an unexpected ELF machine, a dynamic loader or shared-library requirements before checking executable startup. They use the baseline target CPU, without `target-cpu=native`, to avoid requiring the runner’s newer ARM instruction set. Native runner availability is documented by [GitHub](https://docs.github.com/en/actions/reference/runners/github-hosted-runners). This is software coverage, not acceptance on every NAS or physical audio device.

Run Release manually from GitHub Actions to build downloadable artifacts without creating a release. This is the rehearsal for packaging on all four CLI targets and three App platforms.

For a release, commit the intended version and matching lockfile, verify CI, then push the matching version tag (currently `v0.7.5`). The tag must match the workspace version. The workflow attaches archives and SHA-256 checksums to a **draft**. Review its notes and downloads before publishing it in GitHub Releases. Builds are unsigned.

Version-specific highlights live in `docs/releases/vVERSION.md`; the workflow uses them when present and adds download instructions and the commit history. New versions without a notes file receive a reminder to write highlights.

## Website and user documentation

The `Site` workflow builds the complete publication into `dist-site/`: the Onde landing page, English counterpart, user manual, CLI reference, server guide, DSP guide and Rustdoc. Pull requests build and validate an artifact without publishing it. Pushes to the default branch and manual runs publish the same validated artifact to `gh-pages`.

The temporary `aede-website` Actions artifact is retained for one day. This does not change the published website or files attached to Releases. The setting applies to new uploads; older artifacts keep their original expiration dates and must expire or be removed separately. CLI/App release build artifacts retain their separate seven-day policy.

User-facing Markdown is the source of truth. English guides live in `docs/manual/`, `docs/cli/`, `docs/server/` and `docs/dsp/`; translated guides live under `docs/fr/`. The `docs/site-*.json` manifests map titles, descriptions, navigation groups and bilingual paths. Existing topic references are published directly from their Markdown, rather than copied into hand-maintained HTML. Rendering requires Python's standard library and no package installation.

```sh
python3 tools/build_site_tests.py
python3 tools/build-site.py --check
python3 tools/preview-site.py
```

The output is ignored by Git. Rustdoc is built separately with the Rust toolchain and copied to `dist-site/docs/rust/`; `tools/check.sh` checks the combined publication. See [site maintenance](../../site/README.md) for preview, translation and form configuration. The signup form uses a native POST to FormSubmit, which requires the recipient to activate their address after the first submission; website builds do not send email or activate the recipient.

## Locked source checks without product builds

`bash tools/check.sh --sources-only` runs the root helper/site/release-contract tests, formatting, Clippy, all default Rust tests, the shared-audio all-feature/minimal/decode-only profiles, DSP without loudness, terminal regressions and generated documentation checks. It omits the optimized executable build. The independent desktop workspace is not a root workspace member and is not compiled. The same mode in FlacCompagnon and Phémios tests every non-App workspace package and frontend types/behavior without Vite/native/sidecar preparation. These are host source checks, not product, installed-engine, cross-platform or hardware acceptance.

A reviewed FlacCompagnon full revision can be committed before stable publication. `tools/update-flaccompagnon.py` still selects only published stable releases and can replace this revision with the selected stable tag; a failed update retains the prior manifest and lock. Source validation consumes the existing lock and never calls that online preparation step. Publish the coordinated sources before expecting the normal latest-stable product workflow to consume new native APIs.
