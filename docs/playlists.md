# Playlists in the folders

```sh
aede playlist                       # an .m3u in every album folder
aede playlist ~/Music/Ozzy          # only under that folder
aede playlist --simple              # paths only, no #EXTINF
aede playlist --artists             # and one per artist, their whole discography
aede playlist --dry-run             # say what it would write, write nothing
```

With no folder it writes into every album folder of the library. That is fast — a few kilobytes of text per album, no decoding — but it does touch a lot of folders, so `--dry-run` first is the way to see the extent of it.

The file is named after its **folder** — `1959 Kind of Blue [FLAC].m3u` — not after the album title. Two folders can hold the same title (a rip and a remaster), and a title carries `/` and `:` on records named by people rather than by filesystems; the folder name is unique where the file goes and already legal there.

Paths are **relative**, which is the whole point of a playlist that lives beside its music: the folder can be moved, copied to a card or read on another machine and the playlist still plays. (`--m3u` on a selection keeps absolute paths — that file may be saved anywhere and has no folder to be relative to. Both go through the same renderer, so they cannot drift apart on what an `#EXTINF` line looks like.)

The order is the album's own — disc, then track number, from the tags. **A box set laid out as `Disc 1`, `Disc 2` gets one playlist in the parent folder, spanning the discs**, which is exactly the file wanted when the tracks are numbered 1..17 twice over.

`--artists` infers the artist folder as the one every album of that artist sits in. Where they do not share one, or share only a watched root, nothing is written: a library laid out flat would otherwise get one playlist per artist dumped at its top, which is littering rather than tidying.

**A second run writes nothing.** The test is on the _text_: a playlist is derived from the set of tracks, not from their bytes, so adding a track changes what it should say without touching any file it already names. Comparing the rendered text answers both questions at once, and leaves the file's date alone when nothing changed — which matters to whatever syncs the folder afterwards.
