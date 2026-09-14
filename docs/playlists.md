# Playlists: Living in the Folders

Generating playlists for an entire CDthèque shouldn't be a tedious manual chore. The `aede playlist` command gracefully walks your library, pressing a ready-to-play `.m3u` file directly into the physical folders where your audio resides.

```
aede playlist                       # an .m3u in every album folder
aede playlist ~/Music/Ozzy          # only under that specific path
aede playlist --simple              # paths only, stripping the #EXTINF metadata
aede playlist --artists             # add one master playlist per artist discography
aede playlist --dry-run             # preview the sweep without writing a single byte
```

When run without a specific folder argument, it writes into every album folder across your entire library. It is incredibly fast—producing just a few kilobytes of text per album with zero audio decoding required. However, because it touches so many directories, running `--dry-run` first is a wise habit to gauge the exact scope of the operation.

## Naming and Portability: The Archivist's Rules

A playlist file generated this way is named after its **folder**—e.g., `1959 Kind of Blue [FLAC].m3u`—rather than the album title in the metadata tags.

This is a deliberate, defensive design choice:

- **Filesystem Legality:** An album title like _1/2: The Early Years_ contains characters (`/`, `:`) that are strictly forbidden by operating systems. A folder name is already guaranteed to be legally permitted exactly where the playlist is being written.
- **Uniqueness:** Two distinct folders can hold albums with the exact same title (e.g., an original CD rip and a high-res remaster). Naming the playlist after the folder ensures neither overwrites the other.

Crucially, the paths inside these playlists are strictly **relative**. The entire point of a playlist living beside its music is portability: you can move the folder to a new drive, copy it to a micro-SD card, or share it over a network, and the playlist will still function perfectly.

_(Note: This is the exact opposite of using `--m3u` on a curated `aede search` or `aede artist` selection, which generates absolute paths so the exported file can be saved and played from anywhere on your system. Both methods use the same internal renderer, ensuring the `#EXTINF` duration and title lines remain perfectly consistent.)_

## Box Sets and Discographies

The track order respects the tags: disc number first, then track number.

**Box sets are handled with elegance.** If a release is laid out across subfolders like `Disc 1` and `Disc 2`, Aède intelligently places a single, unified playlist in the **parent folder** spanning all discs. This is exactly what a curator wants when faced with tracks numbered 1 through 17 twice over.

When `--artists` is passed, Aède attempts to create a master playlist for an artist's entire discography. It does this by inferring the artist's root folder—the one directory that contains every album by that artist. If the albums are scattered, or if they only share a top-level watched root (like `~/Music`), Aède politely refuses to write the artist playlist. A library laid out completely flat would otherwise suffer a dump of hundreds of artist playlists at its root, which is digital littering rather than thoughtful organization.

## The Second Run: Silent and Safe

**A second run writes nothing.**

Aède does not simply check if the `.m3u` file exists; it tests the _text_. A playlist's truth is derived from the current set of tracks in the catalog, not from the physical bytes on disk. If you add a missing track to an album and re-run the command, Aède compares the newly rendered text against what is already on disk.

If the text matches perfectly, the file is left completely alone. This preserves the file's original modification date—a vital detail for backup software and synchronization tools that rely on timestamps to know when an archive has truly changed.
