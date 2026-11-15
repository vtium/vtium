# Cullet Mono

Cullet Mono is vtium's outline version of the Terminus font, with Nerd Fonts' icons added. It is the outline font of vtium's default font set, `terminus`, for character cells taller than 16 pixels and for Nerd Fonts icons at every size. This tool builds it from two pinned sources: Terminus (TTF) 4.49.3 by Tilman Blumenbach, and Nerd Fonts' font patcher 3.4.0. The patcher adds every icon set it offers apart from Font Logos, whose icons carry no license. Terminus (TTF)'s license reserves the names "Terminus Font" and "Terminus (TTF)", so the result has a name of its own. A cullet is the broken glass that a glassmaker melts down again with new glass.

## Building it

Run the build from the repository root:

```sh
make cullet-mono
```

The build needs:

- [FontForge](https://fontforge.org/), the font editor that runs the patcher. The pins below were made with FontForge 20251009.
- Python 3, which unpacks the two source archives.
- curl, and the network the first time. The build fetches the sources into `$XDG_CACHE_HOME/vtium/cullet-mono` and uses that copy from then on.

The build takes about 25 seconds. It writes into `target/cullet-mono/`:

| File | What it holds |
|---|---|
| `CulletMono-Regular.ttf`, `CulletMono-Bold.ttf` | the two faces |
| `notices/` | `NOTICES.txt` and the license texts it names, copied from `notices/` here |
| `build.tsv` | the FontForge and patcher versions, the date written into each face, and each face's size, character count and SHA-256 |
| `logs/` | the patcher's output for each face |

`make cullet-mono` is not a gate, because it needs FontForge and the network.

## What the build checks

The build stops on a source whose SHA-256 differs from its pin, and removes that file from the cache. It then checks each face it writes, and fails if any check fails:

- The family name is "Cullet Mono", and the style and full name are the face's own.
- No string uses "Terminus Font" or "Terminus (TTF)" as the font's name. The copyright and license strings of the `name` table may mention them.
- Each face maps 11,638 characters, and none of them lies in U+F300 to U+F385, the Font Logos icons.
- The patcher reports adding exactly the glyph sets that `NOTICES.txt` lists.
- Each license file in `notices/` that comes from a source archive still matches the archive's copy.
- Each face has the size and SHA-256 that `pins.tsv` pins.

## Checking a build

A build matches when both faces have the SHA-256 that `pins.tsv` pins. To check faces built earlier, without building them again, run:

```sh
cargo run --release --manifest-path tools/cullet-mono/Cargo.toml -- check target/cullet-mono
```

This runs every check above on the two faces in that folder. `sha256sum target/cullet-mono/*.ttf` shows the same SHA-256 values.

## How the build stays reproducible

Two builds from the same sources with the same FontForge give the same bytes. Three things would otherwise change between builds:

- FontForge writes the time of the build into the `head` table's modification date, and the table's checksum changes with it. The build sets `SOURCE_DATE_EPOCH` to 1745518876 (24 April 2025, 18:21:16 UTC), the date of the Nerd Fonts commit the patcher comes from.
- The patcher adds the output of `git describe` to its version when it runs inside a git checkout, which the output folder usually is. The build points `GIT_DIR` at a path that does not exist, so git finds no checkout.
- The patcher is a Python program. The build sets `PYTHONHASHSEED=0`, so that the order of any set of strings it walks is the same in every run.

Another version of FontForge may give other bytes. The build then fails on the SHA-256 check, and the message names the FontForge version that `pins.tsv` was made with. To move to a new FontForge or a new source, change `pins.tsv` in a commit of its own that lists each value that moved.

## pins.tsv

`pins.tsv` holds one row for each source and each face, with five tab-separated columns:

| Column | What it holds |
|---|---|
| `kind` | `source` for a file the build fetches, `face` for a file it writes |
| `file` | the file's name, in the cache or in the output folder |
| `bytes` | the file's size |
| `sha256` | the SHA-256 of the file's bytes |
| `from` | a source's URL, or the FontForge version that built a face |

## The notices

`notices/NOTICES.txt` lists each part of Cullet Mono with its copyright, its license, and the attribution that license asks for. It covers Terminus (TTF) with its two Reserved Font Names, the patcher under the MIT License, and each icon set. The icon sets' licenses come from Nerd Fonts' `license-audit.md` at v3.4.0, and the patcher's own notes give each set's version and changes. The other files in `notices/` hold the full license texts. The ones that come from a source archive are verbatim copies, and the build checks them against the archive.

## Files

| Path | What it is |
|---|---|
| `pins.tsv` | the pinned sources and the expected faces |
| `notices/` | the notices and license texts that go beside the faces |
| `src/recipe.rs` | the patcher's options, the faces' names, and what the checks expect |
| `src/build.rs` | fetching, unpacking, patching and recording the build |
| `src/check.rs` | the checks on each face |
| `src/sfnt.rs` | reads the font file's table directory, `name` table and `cmap` table |

The tool is its own Cargo workspace with no dependencies, so the main workspace and CI do not build it. It hashes with vtium-font's SHA-256 code, which it includes by path.
