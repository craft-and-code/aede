# Multiple-audio MP4 fixture

`multi-audio.m4a.bin` is a real MP4 containing two mono audio tracks at 44,100 Hz: 16-bit ALAC followed by AAC. Both encode the same generated 50 ms sine wave; no third-party music is used. The `.bin` suffix keeps unrelated fixture-library scans from treating this metadata control as a single-track album.

```sh
ffmpeg -hide_banner -loglevel error -f lavfi -i sine=frequency=440:sample_rate=44100:duration=0.05 -map 0:a -map 0:a -c:a:0 alac -c:a:1 aac -map_metadata -1 multi-audio.m4a
ffprobe -v error -show_entries stream=index,codec_name,sample_rate,channels,bits_per_raw_sample -of json multi-audio.m4a
```

The metadata reader retains its existing final-audio-description selection. The regression checks that the final AAC description cannot inherit the preceding ALAC description's PCM precision. It does not add alternate-track selection or multi-track playback support.
