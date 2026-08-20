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
    scanned_at        INTEGER,
    -- What the last integrity check concluded, or NULL when none was ever run.
    -- NULL is not the same as 'nothing_to_check': the first can change, the
    -- second is final for that container.
    integrity_state    TEXT CHECK (integrity_state IN ('intact', 'damaged', 'nothing_to_check')),
    -- How the verdict was reached: flac-frame-crc, ogg-page-crc, none. A frame
    -- checksum proves the container intact; an MD5 of the decoded audio, which
    -- will come with the decoder, proves the music itself intact.
    integrity_method   TEXT,
    integrity_detail   TEXT,           -- where it failed, when it did
    integrity_checked_at INTEGER       -- Unix epoch in seconds
);

CREATE INDEX file_integrity_idx ON file (integrity_state);

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

-- What another tool measured by decoding the audio, imported and attributed.
--
-- Deliberately a table of its own rather than columns on `file`: these values
-- were not obtained by Aède's own method, and a verdict carries the method
-- that produced it. Keeping them apart is what lets `doctor` notice that two
-- methods disagree — a `Mismatch` here against an `intact` on `file` says the
-- container is sound but the audio inside was re-encoded.
--
-- `size_bytes` and `modified_unix` are the file as it was when it was
-- analysed, and expire the row exactly as the incremental scan expires a read.
-- Every measurement is nullable: NULL is "not measured", which for a peak or a
-- dynamic range is not the same as zero.
--
-- Keyed on the **path** and deliberately not a foreign key onto `file`: a row
-- may describe a file the catalog does not hold yet. Analysing a folder before
-- ever scanning it is a legitimate order of operations, and such a row waits
-- here until the scan brings its file in. `v_file_analysis` is the join for
-- everything else.
CREATE TABLE analysis (
    path             TEXT    NOT NULL,   -- absolute path the tool analysed
    source           TEXT    NOT NULL,   -- 'flaccompagnon'
    source_version   INTEGER NOT NULL,   -- version of that tool's report format
    imported_at      INTEGER NOT NULL,   -- Unix epoch in seconds
    size_bytes       INTEGER NOT NULL,
    modified_unix    INTEGER NOT NULL,
    -- Verdict on the MD5 the encoder wrote in STREAMINFO, as a state and not a
    -- hash: the tool compares and reports. 'Match' is a successful `flac -t`.
    -- Kept in the source's own spelling, unnormalised, because the row belongs
    -- to the source: renaming its vocabulary here would make the catalog claim
    -- a verdict the tool never worded that way.
    md5_state        TEXT CHECK (md5_state IN
                         ('NoSignature', 'Present', 'Match', 'Mismatch', 'Error')),
    md5_detail       TEXT,
    real_bit_depth   INTEGER,            -- depth actually carried, by decoding
    requant_rate     REAL,
    fake_stereo      INTEGER,            -- both channels hold the same signal
    ext_mismatch     INTEGER,            -- extension against the real container
    transcoding      TEXT CHECK (transcoding IN ('none', 'suspected', 'detected')),
    upscaling        INTEGER,            -- lossless built from a lossy source
    upsampling       INTEGER,            -- rate raised above what the content justifies
    summary          TEXT,               -- the tool's own one-word verdict
    detail           TEXT,               -- and the same in a sentence
    cutoff_hz        REAL,               -- where the spectrum stops
    cutoff_ratio     REAL,               -- as a fraction of the Nyquist limit
    dr_db            REAL,
    peak_dbfs        REAL,
    true_peak_dbtp   REAL,
    clipped_samples  INTEGER,
    clip_events      INTEGER,
    clipped          INTEGER,
    error            TEXT,               -- what stopped the analysis of this file
    -- One analysis per path and per source: importing the same report twice
    -- replaces, it does not accumulate.
    PRIMARY KEY (path, source)
) WITHOUT ROWID;

CREATE INDEX analysis_md5_idx ON analysis (md5_state);

-- Imported analyses that describe a file the catalog actually holds, and only
-- those whose row still applies to it. A row that fails the size and date test
-- describes bytes that are no longer there and must never reach a query.
CREATE VIEW v_file_analysis AS
SELECT f.id AS file_id, a.*
FROM analysis a
JOIN file f ON f.path = a.path
WHERE a.size_bytes = f.size AND a.modified_unix = f.mtime;

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
