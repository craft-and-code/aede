# Spectrograms: The Ultimate Acoustic Truth

A spectrogram is a visual diagnostic tool when a file’s origin is uncertain. Metadata can be incorrect or misleading. A FLAC file transcoded from MP3 may still report lossless codec or container properties, while its spectrum can provide additional clues about its audio history. A spectral cutoff around 16 kHz alone does not prove transcoding: the original material and earlier processing can also limit bandwidth. Neither a spectrogram nor tags alone guarantee a lossless origin or establish complete provenance.

Aède provides the microscope to see exactly what you are archiving.

```sh
aede spectrum                       # reveal the acoustic truth of the entire catalog
aede spectrum ~/Music/Ozzy          # focus the microscope on a specific shelf
aede spectrum --dry-run             # preview the effort without drawing a single pixel
aede spectrum --full                # fiercely redraw everything, overriding current files
aede spectrum --size full           # match FlacCompagnon's exact dimensions for direct comparison
```

## The Physical Toll of Acoustic Analysis

**With no folder specified, Aède meticulously analyzes the entire library.** For a vast CDthèque, this is a monumental mathematical effort. Every single track must be fully decoded, and an intricate Fast Fourier Transform (FFT) is swept across the audio. At several seconds per track, a sprawling archive of tens of thousands of tracks requires hours to visualize, regardless of how many processor cores are engaged.

Because of this physical toll, running `--dry-run` first is the archivist's standard habit to gauge the exact scope of the operation. Naming a specific folder (e.g., `~/Music/Ozzy`) is the surgical way to focus the heavy lifting strictly on newly acquired or suspicious rips.

There is zero penalty for stopping halfway through a massive run. If you cancel the command, Aède elegantly picks up exactly where it left off on the next run, leaving everything already drawn perfectly intact.

## Visual Provenance and FlacCompagnon

Each rendered image is deposited as a PNG into a dedicated `spectrograms/` subdirectory, sitting immediately beside the master audio files it describes.

Crucially, these are drawn using the **exact same ffmpeg filter and colour map as [FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/)**. This is a deliberate, uncompromising design choice. An archivist often relies on both tools, and spectrograms that differ in gain or colour mapping from one program to the other would be impossible to read _as a pair_. We ensure the acoustic reality looks mathematically identical across your entire forensic suite.

## Frame Size and Storage Footprint

The dimensions of the frame are left to the curator's discretion:

- **`--size half` (The Default):** Draws a picture at `900x470`—exactly a quarter of the pixels of FlacCompagnon's original `1800x940`. These smaller dimensions can reduce storage use, but the encoded PNG size and the amount saved depend on the image content and compression. There is no fixed quarter-size guarantee and no guarantee that an archive of thousands of tracks will remain within megabytes rather than gigabytes.
- **`--size full`:** Reproduces the exact FlacCompagnon dimensions for flawless, pixel-perfect side-by-side analysis.

Note that switching the `--size` flag does not automatically redraw what is already safely in the vault. An image is only redrawn when it is missing or out of date. To force a library-wide resize, you must use `aede spectrum --full`.

## Orchestration and Dependencies

Aède harnesses your machine's full potential by running several tracks concurrently. Because drawing a picture requires decoding the file and computing the FFT independently, no two pictures share any state. The `--threads` flag dictates exactly how many parallel microscopes are active, sharing the same logic used in `aede scan`.

While Aède orchestrates the analysis, it relies on an external engine to decode the audio. **FFmpeg must be installed** on your system (`brew install ffmpeg` on macOS, or `sudo apt install ffmpeg` on Debian/Ubuntu). Aède checks for this dependency exactly once before touching the first file, delivering a single, polite notification if it is missing, rather than flooding your terminal with an error for every track.

## The Second Run: Silent and Safe

**A second run over an unchanged library draws absolutely nothing.**

A picture is only redrawn if it is completely absent, or if the audio track's modification timestamp has moved past the picture's creation date. This vital check is read directly from the physical disk rather than the catalog. It constantly asks the only question that matters: _was this visual record drawn from the exact bytes that exist here right now?_
