# play — Play local music

play accepts an existing file, recursively ordered folder, M3U/M3U8, saved collection or catalogued name. Use collection:NAME to select a collection explicitly. Relative playlist entries resolve beside the playlist, not the terminal’s working directory. File/folder playback can start without scanning; catalog names and collections need the catalog.

On a macOS/Linux terminal: Space pauses/resumes, n or Right skips forward, p or Left goes back (or restarts after three seconds), q or s stops. Without terminal input, the selection advances automatically. Windows terminal transport keys are not implemented. Listening history is recorded in personal data.

Album normalization is the default for a catalogued album; other selections use track normalization toward -18 LUFS. Current FlacCompagnon LUFS/true-peak results, tags or cached measurements can supply gain. Missing loudness is measured during playback, without changing the current track’s level midway. Bass/treble boosts reserve headroom. CPAL uses a compatible native output where available, otherwise ffplay; Opus/AAC/ALAC decoding can need ffmpeg. Linux release archives require ffplay. The meter and output guard report sample overloads; there is no dynamic limiter or guarantee of a true-peak ceiling. Hardware gapless behavior remains unmeasured.

AEDE_AUDIO_BACKEND chooses local output: unset tries native then ffplay; native requires native output and refuses instead of falling back; ffplay explicitly selects that program. Other values are refused. For example, on macOS/Linux: `AEDE_AUDIO_BACKEND=ffplay aede play "/path/to/track.flac"`. On PowerShell, set `$env:AEDE_AUDIO_BACKEND = "ffplay"` before running play. This selects the output backend, not the decoder or DSP quality.

## Syntax and arguments

```text
aede play <file|folder|m3u|collection|artist|album|track> [--normalize off|track|album] [--bass DB] [--treble DB]
```

One selection; quote names/paths containing spaces. Prefix a saved collection with collection:.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--normalize off\|track\|album` | Choose off, track or album loudness normalization. Default album for a catalogued album selection, track for other selections. |
| `--bass DB` | Broad bass shelf, -12 to +12 dB. 0 is flat; positive boosts reserve headroom. |
| `--treble DB` | Broad treble shelf, -12 to +12 dB. 0 is flat; positive boosts reserve headroom. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede play "$HOME/Music/album/01.flac"
aede play "Kind of Blue" --normalize album
aede play collection:Road --bass 2 --treble -1
aede play "$HOME/Music/album/album.m3u" --normalize off
```

## Result and errors

The terminal names the album and numbered filename as playback advances, shows 24 animated spectrum bars, and reports active DSP stages, normalization/tone headroom and output-meter values. The meter observes guarded PCM submitted to local output before dither/device conversion, not sound measured at the loudspeaker. Read normalization source and guard intervention reports when interpreting level changes. Natural completion returns to the shell; Next after the final track and Stop also end playback. A missing decoder/output device, unsupported channel layout, malformed playlist or file decode/output failure returns a diagnostic; completed listening records may already be saved. A device shortage/underrun is an output problem, distinct from a catalog tag issue. See the DSP guide for measurement limits.

## Related reading

[track](track.md), [analyze](analyze.md), [history](history.md).

Detailed existing guide: [design/playback.md](../design/playback.md).
