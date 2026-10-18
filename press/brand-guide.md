# Aède — visual guide

## Naming and assets

Use **Aède**, preserving its accent. The phonetic desktop title is distinct from the written product name. Use the approved SVG icon and wordmark from `logos/`. The wordmark dark variant contains light lettering; its light variant contains dark lettering. The app icon retains its own dark tile and is identical on either background. Preserve the accent dot, proportions and clear space of at least one dot diameter. Minimum recommended display size: icon 24 px; wordmark 120 px wide. Do not crop the dot, distort, outline, recolour or claim a third-party partnership.

## Palette

| Role | Hex |
| --- | --- |
| Background | `#111213` |
| Surface | `#1A1C1D` |
| Text | `#F1F0EB` |
| Accent / dot / active controls | `#D8F36A` |
| Muted text | `#A4A6A7` |
| Border | `#303234` |

Use the palette SVG for print/layout preparation. The app allows user-custom colours; editorial screenshots should state when customised. See `fonts/README.md` for typography and redistribution terms.

## Editorial asset permission

Project-authored logos, illustrations and screenshots in this kit may be reproduced to report on or review Aède, with attribution “Aède / Craft & Code”. This is permission for editorial reproduction, not transfer of the name or an endorsement. Font assets keep their separate OFL licence. Do not imply that third-party album art or artist photography is licensed by this kit; supplied visuals use fictional data or original illustrations. Confirm publication timing, public downloads and supported release platforms with the maintainer rather than inferring them from development screenshots.

The documentation wordmark is the shape reference across the site, App and press kit. Outlined Helvetica Neue Bold preserves its heavier rendering without distributing the proprietary font. Both projects share this weight, close accent dot and unchanged icon background.

The desktop icon source is `desktop/public/icon.svg`, also supplied in the press kit. From `desktop/`, `npm exec tauri -- icon public/icon.svg --output src-tauri/icons` regenerates the platform assets. Both icon variants retain the same dark tile and light initial; only wordmarks invert their letter colour on light backgrounds.
