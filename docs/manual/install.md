# Install Aède

Aède is a command-line executable named `aede` (`aede.exe` on Windows). You run it in Terminal, a Linux terminal or PowerShell. No graphical application or background service is installed automatically. The website documents the current repository; an older downloaded release may expose fewer commands. Check `aede --version` and its bundled help when comparing versions.

## Choose a distribution

The release workflow prepares the following archives. Download an actually published asset from [GitHub Releases](https://github.com/craft-and-code/aede/releases); a workflow target does not mean that a matching release is already published. Choose your computer’s processor, not the processor of a different NAS.

| System | Archive label | Playback notes |
| --- | --- | --- |
| macOS, Apple Silicon | `macOS-AppleSilicon` | Native CPAL output when compatible, ffplay fallback |
| Linux, x86_64 | `Linux-x86_64` | Release archive uses musl and needs ffplay for playback |
| Windows, x64 | `Windows-x64` | Native path scanning/copying tested in Windows CI; terminal transport controls/local writer delegation remain Unix-only |

Future releases do not include a prebuilt macOS Intel archive. There is no packaged Linux ARM/Raspberry Pi release or published Docker image in the current workflow. Building on another target requires its own validation. The existing server is a local catalog API, not an audio streamer.

## Run a downloaded executable

Extract the archive and its executable. On macOS/Linux, open a terminal in the extracted folder:

```sh
./aede --version
./aede help
./aede scan "$HOME/Music"
```

`./` means “run the file in this folder”. If you later add that folder to your shell’s `PATH`, you can write `aede` from any folder. Do not put the executable or data directory inside a folder that will be erased when updating a release.

On Windows, extract the ZIP, open PowerShell in that folder and run:

```powershell
.\aede.exe --version
.\aede.exe help
.\aede.exe scan "$env:USERPROFILE\Music"
```

The archives are unsigned. Check their adjacent SHA-256 file before trusting the download: `shasum -a 256 ARCHIVE.tar.gz` on macOS, `sha256sum ARCHIVE.tar.gz` on Linux, or `Get-FileHash ARCHIVE.zip -Algorithm SHA256` in PowerShell, then compare the value with the downloaded checksum. macOS may quarantine unsigned downloads. Once you have checked the source and checksum, the release procedure documents `xattr -dr com.apple.quarantine ./aede` for that specific executable; do not remove quarantine from unrelated downloads.

## Build the current source

Use this route when you need repository features newer than a published release. Install Git and Rust through [rustup](https://rustup.rs/). The project requires Rust 1.89 or later and downloads dependencies, including a pinned FlacCompagnon library.

```sh
git clone https://github.com/craft-and-code/aede.git
cd aede
cargo build --release --locked -p aede-cli
./target/release/aede --version
```

On Debian/Ubuntu GNU/Linux, native audio compilation also needs `pkg-config` and ALSA development headers:

```sh
sudo apt install pkg-config libasound2-dev
```

On Windows, install Rust’s MSVC toolchain and the Microsoft C++ build tools required by it, then use the same Cargo build command in PowerShell. The result is `.\target\release\aede.exe`. Source compilation on macOS needs its system developer/linker tools; install the command-line tools if the linker is missing. The build above respects the lockfile; the project’s online `tools/build.sh` is a developer workflow that can update the FlacCompagnon dependency and run the full checks.

## Optional audio tools

Scanning, browsing, queries, annotations, backups and the local catalog API do not require FFmpeg. Install `ffmpeg`/`ffplay` for conversion, applicable spectrogram/decoder paths and fallback playback. Opus/AAC/ALAC decoding can need FFmpeg. Native tag reading alone does not guarantee every format can be decoded by the player.

```sh
# macOS with Homebrew
brew install ffmpeg
# Debian/Ubuntu
sudo apt install ffmpeg
```

On Windows, obtain a build from the distribution links on [FFmpeg’s download page](https://ffmpeg.org/download.html) and add its `bin` folder to `PATH`. Verify `ffmpeg -version` and `ffplay -version` in the same terminal that runs Aède. Fingerprinting additionally needs `fpcalc` or an FFmpeg build with Chromaprint; installing ordinary FFmpeg does not promise that feature. Service API keys are only needed for the corresponding explicit `fetch` passes.

## Decide where Aède stores data

Music and Aède data are different folders. Music is the original audio. Data contains `catalog.json`, `conclusions.json`, `user.json`, `sources.json` and derivative assets. By default the executable checks, in order: `--data FOLDER`, `AEDE_HOME`, `$XDG_DATA_HOME/aede`, then `~/.local/share/aede`; if `HOME` is unset it falls back to `.aede` in the working folder. This same resolution applies to all platforms, so explicitly choosing a Windows data path avoids relying on a shell’s Unix-style environment variables.

```sh
aede scan "$HOME/Music" --data "$HOME/aede-data"
aede stats --data "$HOME/aede-data"
```

```powershell
.\aede.exe scan "D:\Music" --data "$env:LOCALAPPDATA\Aede"
.\aede.exe stats --data "$env:LOCALAPPDATA\Aede"
```

Always use the same data location for cooperating CLI/server commands. `--data` does not mean “scan this music folder”. Continue with [first steps](first-steps.md) and [data safety](catalog.md).
