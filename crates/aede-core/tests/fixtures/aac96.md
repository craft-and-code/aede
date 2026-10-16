# High-rate AAC/M4A metadata fixture

`aac96.m4a.bin` contains a one-second AAC derivative of the existing synthetic `hires.flac` test fixture. It contains no newly supplied recording. Generated with FFmpeg 9.0.2:

```sh
ffmpeg -hide_banner -loglevel error -nostdin -i hires.flac -c:a aac -ar 96000 aac96.m4a
ffprobe -v error -select_streams a:0 -show_entries stream=codec_name,sample_rate,channels -of json aac96.m4a
mv aac96.m4a aac96.m4a.bin
```

FFprobe reports AAC, 96000 Hz, one channel. The MP4 AudioSampleEntry carries a 48000 Hz compatibility value; its `esds` decoder-specific configuration carries the actual 96000 Hz rate. This exercises the [limit of that unsigned 16.16 sample-entry field](https://developer.apple.com/documentation/quicktime-file-format/sound_sample_description_version_1/sample_rate) without adding a codec or changing decoded audio support. Ordinary-rate AAC and ALAC keep their existing controls.

SHA256: `1bf594f3967a5557e2f59bf9b639a2611ca5d342dec246a71a3398ea0972d113`.

The `.bin` suffix keeps this dedicated regression out of the shared CLI reference-library scan. The sibling test writes its unchanged bytes to an isolated `.m4a` path before reading both public metadata entry points.
