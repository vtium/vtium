# The font files every build installs

Every vtium build, the minspec builds included, installs its own font files in `/usr/share/vtium/fonts/`. The default font set, `terminus`, draws from the first two, and the outline set, `cullet-mono`, from the third:

| Files | What they are | Bytes |
|---|---|---|
| `ter-u12n.bdf.gz` to `ter-u32b.bdf.gz` | bitmap Terminus 4.49.1's eighteen BDF files, regular and bold at 12, 14, 16, 18, 20, 22, 24, 28 and 32 pixels, gzipped | 327,314 |
| `unifont_all-17.0.03.hex` | GNU Unifont 17.0.03, every plane, uncompressed | 8,329,308 |
| `CulletMono-Regular.ttf`, `CulletMono-Bold.ttf` | Cullet Mono, vtium's outline Terminus with Nerd Fonts' icons | 5,234,744 |

## Staging them

Run these from the repository root before `make install` or `make install-tree`:

```sh
make font-files
make cullet-mono
```

`make font-files` stages the Terminus and Unifont files in `target/font-files/`, or `$CARGO_TARGET_DIR/font-files/` when that is set. It takes Terminus's release archive, `terminus-font-4.49.1.tar.gz`, from `$XDG_CACHE_HOME/vtium/fonts/`. When the archive is missing there, it downloads the archive with curl, so it needs the network once. It gzips each BDF file with `gzip -n -9`, which leaves the file's name and time out of the gzip header, so the same gzip gives the same bytes on every machine. Unifont's file is a copy of the tree's `crates/vtium-font/data/unifont.hex` under the name Unifoundry gives it.

`make cullet-mono` builds Cullet Mono's two faces in `target/cullet-mono/`. It needs FontForge and Python. [tools/cullet-mono/README.md](../cullet-mono/README.md) describes that build.

`make install` and `make install-tree` check every file against its pin before they stage anything. When a file is missing or differs from its pin, the install stops with a line that names the file and the target that stages it. `make font-files-check` runs the same check alone.

A package recipe that builds with no network puts the archive in the cache itself. The recipe downloads `terminus-font-4.49.1.tar.gz` as a pinned source and links it into `$XDG_CACHE_HOME/vtium/fonts/` before it runs `make font-files`.

## pins.tsv

`pins.tsv` holds one row for each file, with five tab-separated columns:

| Column | What it holds |
|---|---|
| `kind` | `source` for the archive the script downloads, `gzip` for a file it gzips from the archive, `copy` for a file it copies from the tree |
| `file` | the file's name, in the cache or in `target/font-files/` |
| `bytes` | the file's size |
| `sha256` | the SHA-256 of the file's bytes |
| `from` | the archive's URL, the member of the archive a file is gzipped from, or the tree's file a file is copied from |

Cullet Mono's faces are pinned in `tools/cullet-mono/pins.tsv`, which the check reads too.

Another gzip may compress the same file into other bytes. The check then fails on that file's SHA-256, and the file must be made with GNU gzip. The pins were made with GNU gzip 1.15.

## Licenses

Terminus Font is under the SIL Open Font License 1.1, with the Reserved Font Name "Terminus Font". vtium installs its files unchanged apart from the gzip, under their upstream names, and each file states its copyright and license in its `COPYRIGHT` and `NOTICE` properties. vtium takes GNU Unifont under the SIL Open Font License 1.1. A `.hex` file holds glyphs alone, so Unifont's copyright line is in vtium's `COPYING`. Both licenses' text is the `OFL-1.1.txt` that vtium installs beside `COPYING`. Cullet Mono's notices, `NOTICES.txt` and the license texts it names, install in `cullet-mono/` in the package's license directory.

## Files

| Path | What it is |
|---|---|
| `files` | the script: `files fetch OUT` stages the Terminus and Unifont files in OUT, and `files check OUT CULLET` checks them and Cullet Mono's faces |
| `pins.tsv` | the pinned archive and files |
| `test_files.py` | the tests, which `make test-tools` runs with no network |
