# Cullet Mono

Cullet Mono is vtium's outline version of the Terminus font, with Nerd Fonts' icons added. It draws vtium's outline font set, `cullet-mono`, which suits a HiDPI display and can be chosen in place of the default bitmap set, `terminus`. Every build installs both sets, with GNU Unifont. This tool builds it from two pinned sources: Terminus (TTF) 4.49.3 by Tilman Blumenbach, and Nerd Fonts' font patcher 3.4.0. The patcher adds every icon set it offers apart from Font Logos, whose icons carry no license. Terminus (TTF)'s license reserves the names "Terminus Font" and "Terminus (TTF)", so the result has a name of its own. vtium treats the bare word "Terminus" as reserved too: the faces use it only where they state where Cullet Mono comes from. A cullet is the broken glass that a glassmaker melts down again with new glass.

## Building it

Run the build from the repository root:

```sh
make cullet-mono
```

The build needs:

- [FontForge](https://fontforge.org/), the font editor that runs the patcher. The pins below were made with FontForge 20251009.
- Python 3, which unpacks the two source archives.
- curl, and the network the first time. The build fetches the sources into `$XDG_CACHE_HOME/vtium/cullet-mono` and uses that copy from then on.

The build takes about 25 seconds. It writes into `target/cullet-mono/`, or `$CARGO_TARGET_DIR/cullet-mono/` when that is set:

| File | What it holds |
|---|---|
| `CulletMono-Regular.ttf`, `CulletMono-Bold.ttf` | the two faces |
| `notices/` | `NOTICES.txt` and the license texts it names, copied from `notices/` here |
| `build.tsv` | the FontForge and patcher versions, the date written into each face, and each face's size, character count and SHA-256 |
| `logs/` | the patcher's output for each face |
| `.cullet-mono-build` | a marker that says the build writes this folder |

Each build first deletes what an earlier build wrote there. It refuses a folder that holds other files but no marker and no `build.tsv`, so that it never deletes files it did not write.

When a check fails, the build leaves the faces and `build.tsv` in the folder's `work/` subfolder, and it copies no notices. So a failed build never leaves faces that look finished.

`make cullet-mono` is not a gate, because it needs FontForge and the network.

## After the patcher

The patcher renames the font in the `name` table, but FontForge copies each bitmap strike's properties from Terminus (TTF) unchanged. Those properties live in FontForge's `BDF ` table, and FreeType hands them to programs. So the build finishes each face in two steps:

- It renames the family of the bitmap strikes to "Cullet Mono", in the FAMILY_NAME property and in each strike's XLFD name, such as `-xos4-Cullet Mono-Medium-R-Normal--12-120-72-72-C-60-ISO10646-1`. The table's other properties, such as each strike's ascent and default character, stay as they are.
- It writes a description into the `name` table that credits the fonts Cullet Mono is derived from, and their authors.

The description reads:

> Cullet Mono is derived from Terminus Font by Dimitar Toshkov Zhekov and Terminus (TTF) by Tilman Blumenbach, under the SIL Open Font License, Version 1.1. It adds icons from Nerd Fonts.

Every other table keeps the bytes the patcher wrote.

## What the build checks

The build stops on a source whose size or SHA-256 differs from its pin, and removes that file from the cache. It then checks each face it writes, and fails if any check fails:

- The family name is "Cullet Mono", and the style and full name are the face's own.
- The copyright notice is Terminus (TTF)'s, and the description is the one above.
- The word "Terminus", in any letter case, appears only in the copyright notice, the description and the license strings of the `name` table. Any other name, or any other table, that holds the word fails the build.
- Each face maps 11,638 characters, and none of them lies in U+F300 to U+F385, the Font Logos icons.
- The patcher reports adding exactly the glyph sets that `NOTICES.txt` lists.
- Each license file in `notices/` that comes from a source archive still matches the archive's copy.
- Each face has the size and SHA-256 that `pins.tsv` pins.

## Checking a build

A build matches when both faces have the SHA-256 that `pins.tsv` pins. To check the faces that `make cullet-mono` built, without building them again, run:

```sh
make cullet-mono-check
```

This runs every check of the faces above on the two faces in that folder. `sha256sum target/cullet-mono/*.ttf` shows the same SHA-256 values.

## How the build stays reproducible

Two builds from the same sources with the same FontForge give the same bytes. Four things would otherwise change between builds:

- FontForge writes the time of the build into the `head` table's modification date, and the table's checksum changes with it. The build sets `SOURCE_DATE_EPOCH` to 1745518876 (24 April 2025, 18:21:16 UTC), the date of the Nerd Fonts commit the patcher comes from.
- The patcher adds the output of `git describe` to its version when it runs inside a git checkout, which the output folder usually is. The build points `GIT_DIR` at a path that does not exist, so git finds no checkout.
- The patcher is a Python program. The build sets `PYTHONHASHSEED=0`, so that the order of any set of strings it walks is the same in every run.
- FontForge runs your own Python scripts and plugins, and reads your preferences. The build starts FontForge with `-skippyfile` and `-skippyplug`, in an environment that holds only `PATH`, the C locale and the variables above. Its home folder lies inside the work folder, so nothing in your own setup reaches the build.

Another version of FontForge may give other bytes. The build then fails on the SHA-256 check, and the message names the FontForge version that `pins.tsv` was made with. To move to a new FontForge or a new source, change `pins.tsv` in a commit of its own that lists each value that moved.

## pins.tsv

`pins.tsv` holds one row for each source and each face, with five tab-separated columns:

| Column | What it holds |
|---|---|
| `kind` | `source` for a file the build fetches, `face` for a file it writes |
| `file` | the file's name, in the cache or in the output folder |
| `bytes` | the file's size, which the build checks |
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
| `src/finish.rs` | the description and the bitmap strikes' family, written after the patcher |
| `src/check.rs` | the checks on each face |
| `src/sfnt.rs` | reads the font file's table directory, `name` table and `cmap` table, and writes the file again with tables replaced |

The tool is its own Cargo workspace with no dependencies, so the main workspace and CI do not build it. It hashes with vtium-font's SHA-256 code, which it includes by path.
