# Speaking other tools’ languages

## Speaking other servers' languages

Aède's own API (M2) is the contract, and it should be designed for Aède rather
than for anyone else. Compatibility surfaces are then **translations on top of
it**, never a second core — the moment a foreign API's model reaches into the
catalog, that model has won.

**Subsonic / OpenSubsonic first, and it deserves higher priority than it
sounds.** The original Subsonic API has been frozen since 2019 at version
1.16.1; OpenSubsonic is the community continuation, backwards-compatible in both
directions and actively specified. Between them they are spoken by something
like eighty clients — Symfonium, Supersonic, Feishin, Tempo, Amperfy and the
rest — on every platform there is. Implementing it is the difference between
"Aède has no mobile client" and "Aède has thirty", without writing an app. It is
plausibly the highest return of any single item in this file.

**Jellyfin afterwards, and with lower expectations.** Its API is documented by a
generated OpenAPI specification, but the specification is thin on meaning: one
enormous polymorphic item type, an Emby-era composite authorization header, and
enough undocumented behaviour that real integrations proceed by watching the
official web client's traffic. Clients emulate it routinely; servers emulating
it appear to be rare, which is itself a signal. Worth doing for the clients it
unlocks, worth doing _after_ Subsonic, and worth timeboxing.

_(One correction to the note that prompted this: no evidence could be found that
Navidrome exposes any part of the Jellyfin API. Its documentation and releases
mention only Subsonic 1.16.1 plus OpenSubsonic extensions, and its own private
API for its web interface. The bridges that exist run the other way — Jellyfin
plugins that read from Navidrome.)_

## Converting files

Done, and described where the command is: see [Converting on the way out](../copying.md#converting-on-the-way-out).

The two decisions that mattered more than the feature both held. **ffmpeg is an
external program, not a dependency** — invoked, detected, and its absence
reported in a sentence; the dependency rule in `CLAUDE.md` was not bent. And
**the converted files do not come back in through the front door**: `copy`
refuses a destination inside a watched folder, so the originals stay the
library, which is precisely the trap beets designed around.
