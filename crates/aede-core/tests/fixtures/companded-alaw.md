# Companded A-law WAV fixture

`companded-alaw.wav.bin` contains 400 mono A-law samples at 8,000 Hz (50 ms), encoded from a generated sine wave. No third-party music is used. The `.bin` suffix isolates this metadata control from tests scanning the reference music library.

Generate and inspect it with FFmpeg:

```sh
ffmpeg -hide_banner -loglevel error -f lavfi -i sine=frequency=440:sample_rate=8000:duration=0.05 -c:a pcm_alaw -map_metadata -1 -f wav companded-alaw.wav.bin
ffprobe -v error -show_entries stream=codec_name,sample_rate,channels,bits_per_sample,duration,bit_rate -of json companded-alaw.wav.bin
```

The encoder uses WAV format tag 6 with eight-bit encoded words. Those words do not describe linear PCM precision. The metadata regression keeps the known sample rate, channel count and duration while leaving Aède's PCM bit-depth field absent for this lossy representation. This is a metadata-reading control, not an addition to native playback or lossless conversion support.
