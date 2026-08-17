-- Aède — target relational schema (milestone M1).
--
-- At milestone M0 the catalog lives in memory and is persisted as JSON: every
-- key of the file maps exactly to one table below, and every object to one
-- row. This file is therefore not prospective documentation, it is the
-- contract `store.rs` already honours.
--
-- Guiding principle: the model does not say "an album belongs to an artist"
-- but "entities hold roles towards one another". The `credit` and `relation`
-- tables are the core of the system; everything else is just vocabulary.

PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

-- ---------------------------------------------------------------------------
-- Physical files
-- ---------------------------------------------------------------------------

CREATE TABLE file (
    id                INTEGER PRIMARY KEY,
    path              TEXT    NOT NULL UNIQUE,
    size              INTEGER NOT NULL,
    -- Unix epoch in seconds. Together with `size`, acts as the freshness test
    -- for incremental scans: if both are unchanged, the file is not re-read.
    mtime             INTEGER NOT NULL,
    container         TEXT,           -- flac, mp3, mp4, ogg, wav, aiff
    codec             TEXT,           -- flac, mp3, alac, aac, vorbis, opus, pcm
    sample_rate       INTEGER,
    bit_depth         INTEGER,
    channels          INTEGER,
    duration_ms       INTEGER,
    bitrate_kbps      INTEGER,
    lossless          INTEGER NOT NULL DEFAULT 0,
    has_embedded_art  INTEGER NOT NULL DEFAULT 0,
    scanned_at        INTEGER
);

CREATE INDEX file_mtime_idx ON file (mtime);

-- Raw tags kept as they are. They allow the whole graph to be rebuilt without
-- re-reading the files, and make it possible to roll back an automatic
-- correction.
CREATE TABLE raw_tag (
    file_id  INTEGER NOT NULL REFERENCES file (id) ON DELETE CASCADE,
    key      TEXT    NOT NULL,
    -- A single field may carry several values (several artists).
    position INTEGER NOT NULL DEFAULT 0,
    value    TEXT    NOT NULL,
    PRIMARY KEY (file_id, key, position)
) WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Entities
-- ---------------------------------------------------------------------------

CREATE TABLE artist (
    id             INTEGER PRIMARY KEY,
    name           TEXT NOT NULL,
    sort_name      TEXT,
    -- Normalised key (lowercase, without diacritics or leading article): this
    -- is what brings "The Beatles" and "Beatles, The" together.
    key            TEXT NOT NULL,
    mbid           TEXT UNIQUE,
    kind           TEXT,             -- person | group | orchestra | choir
    disambiguation TEXT,
    -- Imported biography (Wikipedia through Wikidata). The licence is stored
    -- alongside the text: CC BY-SA requires attribution and propagates to
    -- translations.
    bio_text       TEXT,
    bio_lang       TEXT,
    bio_source_url TEXT,
    bio_license    TEXT,
    bio_fetched_at INTEGER
);

CREATE INDEX artist_key_idx ON artist (key);

CREATE TABLE label (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    key  TEXT NOT NULL,
    mbid TEXT UNIQUE
);

CREATE TABLE release (
    id                 INTEGER PRIMARY KEY,
    title              TEXT NOT NULL,
    key                TEXT NOT NULL,
    -- NULL for a multi-artist compilation: that is what defines one.
    album_artist_id    INTEGER REFERENCES artist (id) ON DELETE SET NULL,
    date               TEXT,          -- possibly partial ISO date: 1986-03
    year               INTEGER,
    catalog_number     TEXT,
    barcode            TEXT,
    media              TEXT,          -- CD, Vinyl, Digital Media…
    mbid               TEXT,
    release_group_mbid TEXT,
    is_compilation     INTEGER NOT NULL DEFAULT 0,
    folder             TEXT,
    cover_path         TEXT,
    -- Confidence in the MusicBrainz identification, from 0 to 1. Below a
    -- threshold, the entry is offered for manual validation rather than
    -- applied.
    match_confidence   REAL,
    match_source       TEXT           -- tags | musicbrainz | acoustid | manual
);

CREATE INDEX release_key_idx  ON release (key);
CREATE INDEX release_year_idx ON release (year);

CREATE TABLE release_label (
    release_id     INTEGER NOT NULL REFERENCES release (id) ON DELETE CASCADE,
    label_id       INTEGER NOT NULL REFERENCES label (id)   ON DELETE CASCADE,
    catalog_number TEXT,
    PRIMARY KEY (release_id, label_id)
) WITHOUT ROWID;

CREATE TABLE track (
    id          INTEGER PRIMARY KEY,
    file_id     INTEGER NOT NULL REFERENCES file (id) ON DELETE CASCADE,
    release_id  INTEGER REFERENCES release (id) ON DELETE SET NULL,
    title       TEXT NOT NULL,
    disc_no     INTEGER,
    track_no    INTEGER,
    duration_ms INTEGER,
    isrc        TEXT,
    mbid        TEXT,                -- MusicBrainz recording
    bpm         REAL,
    musical_key TEXT,
    -- Filled in by audio analysis (milestone M2), in relative LUFS.
    replaygain_track_gain REAL,
    replaygain_album_gain REAL
);

CREATE INDEX track_release_idx ON track (release_id, disc_no, track_no);
CREATE INDEX track_file_idx    ON track (file_id);

CREATE TABLE genre (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    key  TEXT NOT NULL UNIQUE,
    -- Keep genre, style and mood apart: Roon mixes them, wrongly.
    kind TEXT DEFAULT 'genre'
);

-- ---------------------------------------------------------------------------
-- The graph
-- ---------------------------------------------------------------------------

-- Who does what, on what. `entity_kind` is either 'track' or 'release'.
CREATE TABLE credit (
    id          INTEGER PRIMARY KEY,
    artist_id   INTEGER NOT NULL REFERENCES artist (id) ON DELETE CASCADE,
    entity_kind TEXT    NOT NULL,
    entity_id   INTEGER NOT NULL,
    -- main, album, composer, conductor, producer, engineer, guitar, drums…
    role        TEXT    NOT NULL,
    position    INTEGER,
    source      TEXT    NOT NULL DEFAULT 'tags',
    UNIQUE (artist_id, entity_kind, entity_id, role)
);

CREATE INDEX credit_entity_idx ON credit (entity_kind, entity_id);
CREATE INDEX credit_artist_idx ON credit (artist_id);

-- Typed links between entities. This is where MusicBrainz relations will
-- land: member_of, founded, signed_to, married_to, collaborated…
CREATE TABLE relation (
    id          INTEGER PRIMARY KEY,
    source_kind TEXT    NOT NULL,
    source_id   INTEGER NOT NULL,
    target_kind TEXT    NOT NULL,
    target_id   INTEGER NOT NULL,
    kind        TEXT    NOT NULL,
    attribute   TEXT,                 -- instrument, role refinement
    begin_date  TEXT,
    end_date    TEXT,
    -- Number of observed occurrences: ranks the links by strength.
    weight      INTEGER NOT NULL DEFAULT 1,
    source      TEXT    NOT NULL DEFAULT 'tags',
    UNIQUE (source_kind, source_id, target_kind, target_id, kind, attribute)
);

CREATE INDEX relation_source_idx ON relation (source_kind, source_id, kind);
CREATE INDEX relation_target_idx ON relation (target_kind, target_id, kind);

CREATE TABLE entity_genre (
    genre_id    INTEGER NOT NULL REFERENCES genre (id) ON DELETE CASCADE,
    entity_kind TEXT    NOT NULL,
    entity_id   INTEGER NOT NULL,
    weight      REAL    NOT NULL DEFAULT 1.0,
    source      TEXT    NOT NULL DEFAULT 'tags',
    PRIMARY KEY (genre_id, entity_kind, entity_id)
) WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Full-text search
-- ---------------------------------------------------------------------------

-- A single index for every entity: search is unified on the interface side.
-- `content` carries the already normalised text (without accents), `display`
-- the original text to show.
CREATE VIRTUAL TABLE search USING fts5 (
    content,
    display UNINDEXED,
    entity_kind UNINDEXED,
    entity_id UNINDEXED,
    tokenize = 'unicode61 remove_diacritics 2'
);

-- ---------------------------------------------------------------------------
-- Convenience views
-- ---------------------------------------------------------------------------

CREATE VIEW v_release_summary AS
SELECT
    r.id,
    r.title,
    r.year,
    COALESCE(a.name, 'Various Artists') AS album_artist,
    COUNT(t.id)                         AS track_count,
    SUM(t.duration_ms)                  AS duration_ms,
    SUM(f.size)                         AS bytes
FROM release r
LEFT JOIN artist a ON a.id = r.album_artist_id
LEFT JOIN track  t ON t.release_id = r.id
LEFT JOIN file   f ON f.id = t.file_id
GROUP BY r.id;

-- Every release an artist appears on, whatever the role: this is the query
-- behind the "click on a musician" navigation.
CREATE VIEW v_artist_appearances AS
SELECT DISTINCT
    c.artist_id,
    COALESCE(t.release_id, c.entity_id) AS release_id,
    c.role
FROM credit c
LEFT JOIN track t ON c.entity_kind = 'track' AND t.id = c.entity_id
WHERE c.entity_kind = 'release' OR t.release_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- Metadata about the catalog itself
-- ---------------------------------------------------------------------------

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;

INSERT INTO meta (key, value) VALUES ('format_version', '1');

CREATE TABLE root (
    path       TEXT PRIMARY KEY,
    added_at   INTEGER,
    scanned_at INTEGER
) WITHOUT ROWID;
