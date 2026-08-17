#!/usr/bin/env bash
# Generates a small demo library, deliberately imperfect: it contains missing
# tags, a duplicate, an incomplete album and an album with mixed formats, so
# that `aede doctor` has something to bite on.
#
# Usage: tools/demo-library.sh [output_folder]
# Requires: ffmpeg.

set -euo pipefail

OUTPUT="${1:-/tmp/demo-music}"

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "ffmpeg is required to generate the demo files." >&2
  exit 1
fi

rm -rf "$OUTPUT"
mkdir -p "$OUTPUT"

# track <path> <codec> <sample_rate> <duration> [ffmpeg metadata...]
track() {
  local path="$1" codec="$2" rate="$3" duration="$4"
  shift 4
  mkdir -p "$(dirname "$path")"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "sine=frequency=${FREQ:-440}:duration=${duration}:sample_rate=${rate}" \
    -af "volume=0.15" -c:a "$codec" "$@" "$path"
}

cover_art() {
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "color=c=0x2a4d69:s=300x300:d=1" -frames:v 1 "$1"
}

echo "Generating into $OUTPUT ..."

# --- Miles Davis: Kind of Blue (FLAC 16/44.1, complete, with cover art) -----
A="$OUTPUT/Miles Davis/1959 - Kind of Blue"
i=1
for title in "So What" "Freddie Freeloader" "Blue in Green" "All Blues" "Flamenco Sketches"; do
  FREQ=$((300 + i * 40)) track "$A/$(printf '%02d' $i) $title.flac" flac 44100 2 \
    -sample_fmt s16 \
    -metadata title="$title" \
    -metadata artist="Miles Davis" \
    -metadata album_artist="Miles Davis" \
    -metadata album="Kind of Blue" \
    -metadata date="1959" \
    -metadata genre="Jazz" \
    -metadata track="$i/5" \
    -metadata disc="1/1" \
    -metadata publisher="Columbia" \
    -metadata composer="Miles Davis"
  i=$((i + 1))
done
cover_art "$A/cover.jpg"

# --- John Coltrane: A Love Supreme (FLAC 24/96, hi-res) --------------------
B="$OUTPUT/John Coltrane/1965 - A Love Supreme"
i=1
for title in "Acknowledgement" "Resolution" "Pursuance" "Psalm"; do
  FREQ=$((320 + i * 30)) track "$B/$(printf '%02d' $i) $title.flac" flac 96000 2 \
    -sample_fmt s32 \
    -metadata title="$title" \
    -metadata artist="John Coltrane" \
    -metadata album_artist="John Coltrane" \
    -metadata album="A Love Supreme" \
    -metadata date="1965" \
    -metadata genre="Jazz" \
    -metadata track="$i/4" \
    -metadata publisher="Impulse!"
  i=$((i + 1))
done
cover_art "$B/folder.jpg"

# --- Metallica: Ride the Lightning (MP3, incomplete: no track 3) ------------
C="$OUTPUT/Metallica/1984 - Ride the Lightning"
for n in 1 2 4; do
  case $n in
    1) title="Fight Fire with Fire" ;;
    2) title="Ride the Lightning" ;;
    4) title="Fade to Black" ;;
  esac
  FREQ=$((200 + n * 50)) track "$C/$(printf '%02d' $n) $title.mp3" libmp3lame 44100 2 \
    -b:a 320k -id3v2_version 3 \
    -metadata title="$title" \
    -metadata artist="Metallica" \
    -metadata album_artist="Metallica" \
    -metadata album="Ride the Lightning" \
    -metadata date="1984" \
    -metadata genre="Thrash Metal" \
    -metadata track="$n/8" \
    -metadata publisher="Megaforce"
done

# --- Multi-artist compilation, with a featuring credit ----------------------
D="$OUTPUT/Compilations/2001 - Legendary Duets"
FREQ=500 track "$D/01 Sous le vent.m4a" alac 44100 2 -sample_fmt s16p \
  -metadata title="Sous le vent" \
  -metadata artist="Garou feat. Céline Dion" \
  -metadata album_artist="Various Artists" \
  -metadata album="Legendary Duets" \
  -metadata date="2001" \
  -metadata genre="Chanson" \
  -metadata track="1/2" \
  -metadata compilation="1"
FREQ=560 track "$D/02 Under Pressure.m4a" alac 44100 2 -sample_fmt s16p \
  -metadata title="Under Pressure" \
  -metadata artist="Queen; David Bowie" \
  -metadata album_artist="Various Artists" \
  -metadata album="Legendary Duets" \
  -metadata date="2001" \
  -metadata genre="Rock" \
  -metadata track="2/2" \
  -metadata compilation="1"

# --- Album with mixed formats (FLAC, Vorbis, Opus, two sample rates) --------
E="$OUTPUT/Bjork/1997 - Homogenic"
FREQ=420 track "$E/01 Hunter.flac" flac 44100 2 -sample_fmt s16 \
  -metadata title="Hunter" -metadata artist="Björk" \
  -metadata album_artist="Björk" -metadata album="Homogenic" \
  -metadata date="1997" -metadata genre="Electronic" -metadata track="1/3"
FREQ=470 track "$E/02 Joga.ogg" libvorbis 48000 2 \
  -metadata title="Jóga" -metadata artist="Björk" \
  -metadata album_artist="Björk" -metadata album="Homogenic" \
  -metadata date="1997" -metadata genre="Electronic" -metadata track="2/3"
FREQ=520 track "$E/03 Unravel.opus" libopus 48000 2 \
  -metadata title="Unravel" -metadata artist="Björk" \
  -metadata album_artist="Björk" -metadata album="Homogenic" \
  -metadata date="1997" -metadata genre="Electronic" -metadata track="3/3"

# --- A duplicate of "So What", filed somewhere else -------------------------
FREQ=340 track "$OUTPUT/Unsorted/so what (copy).flac" flac 44100 2 -sample_fmt s16 \
  -metadata title="So What" \
  -metadata artist="Miles Davis" \
  -metadata album="Jazz Essentials" \
  -metadata date="1997"

# --- Two files with no tags at all ------------------------------------------
FREQ=250 track "$OUTPUT/Unsorted/unknown recording.wav" pcm_s16le 44100 1
FREQ=260 track "$OUTPUT/Unsorted/02 - untagged track.flac" flac 44100 1 -sample_fmt s16

echo "Done: $(find "$OUTPUT" -type f | wc -l) files in $OUTPUT"
