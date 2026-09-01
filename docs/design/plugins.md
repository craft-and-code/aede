# Plugins, if there are any

Nothing here is built. It is written down because the question came up and the answer has a shape, and because half the value of a design note in this repository is that it says what will _not_ be done.

## A plugin is a program, not a library

The precedent is already in the codebase: **ffmpeg is driven as an external process**, never linked. `core/ffmpeg.rs` finds it, and `missing(what)` says so plainly when it is absent. Two commands need it, twenty-three do not care, and nothing about the build changes whether it is there.

That is the model a plugin would follow — an executable speaking JSON on stdin and stdout — and the reasons are not stylistic:

- **Rust has no stable ABI.** A dynamically loaded `.so` or `.dylib` must be built by the same compiler version with the same flags as the host binary. That is not a plugin; it is a recompilation with extra steps and a segfault waiting for the day the two drift apart.
- **A plugin that crashes must not take the program with it.** A separate process gives that for free; a loaded library gives the opposite.
- **Anybody can write one**, in any language, without a Rust toolchain — which is the entire point of having plugins rather than pull requests.

## What that suits, and what it does not

It suits work that is **slow and bound by input and output**, where a process boundary costs nothing next to what is on the other side: a metadata source (Discogs, TheAudioDB), a lyrics provider, an analysis tool whose report Aède reads. These already wait on a network or on a disk; a pipe is not what makes them slow.

It suits nothing on the **hot path**. Storage, query evaluation and playback are out, and storage is the clearest case: the store is consulted by every single command, so a subprocess boundary would mean pushing the whole graph through a pipe to answer `aede stats`. Whatever optional storage ends up meaning, it will not mean a plugin.

## Where a plugin's answers would land

Nowhere new — and this is the part worth noticing now rather than later. [The attributed layer](attribution.md) built for M1 keeps a value with the **source** that produced it, in `sources.json`, beside the tag and never on top of it. A plugin is simply another source name in that field. It gets attribution, a fetch date, a confidence, removal, and `doctor` reporting when it disagrees with a tag or with MusicBrainz — all of it, without a line written for plugins specifically.

So the order is already right: the layer first, sources plugged into it afterwards, whether the source is code in this repository or a program somebody else wrote.

## Optional SQLite is a build, not a plugin

Should a SQLite backend ever be wanted — [Architecture](architecture.md#when-this-becomes-a-database) says when it would be, and that the answer may well be never — the way to make it optional is a **Cargo feature**: one storage trait, two implementations, chosen at compile time, and a second binary on the releases page for those who want it. Ordinary, boring, and it works.

**Docker does not make it optional either**, and it is worth separating the two ideas because they arrive together in conversation. An image contains a binary that was already built one way or the other; the container changes how it is installed, not what is inside it. What Docker _is_ genuinely right for is M2: a server on a NAS is how people actually install this kind of software, and that is a deployment target to plan for on its own merits.
