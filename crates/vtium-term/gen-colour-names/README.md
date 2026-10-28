# vtium-term colour name table generator

Generates vtium-term's `src/terminal/color/names.rs`, the X11 colour names an OSC colour string reads, and copies the `rgb.txt` it reads to `src/terminal/color/rgb.txt`, where the table's test reads it. The source is the `rgb.txt` of X.Org's rgb-1.1.1. Both outputs are committed, so the crate builds without it; rerun this only to move the pinned release, and commit the new pins and the regenerated files on their own. It is its own workspace with no dependencies, so the main workspace and CI never build it.

Each line of `rgb.txt` gives a colour and a name, and each name of several words appears twice, spaced and joined (`dark slate gray`, `DarkSlateGray`). The table holds each name once, lowercased with its spaces removed, sorted, with its colour, so a lookup that folds case and drops spaces finds both. A name two lines give two colours stops the generator.

Fetch the release, check its signature, and run it from the repository root:

```sh
mkdir -p /tmp/rgb && cd /tmp/rgb
curl -fsSLO https://www.x.org/releases/individual/app/rgb-1.1.1.tar.xz
curl -fsSLO https://www.x.org/releases/individual/app/rgb-1.1.1.tar.xz.sig
gpg --verify rgb-1.1.1.tar.xz.sig rgb-1.1.1.tar.xz
tar xJf rgb-1.1.1.tar.xz
cd -
cargo run --manifest-path crates/vtium-term/gen-colour-names/Cargo.toml -- /tmp/rgb/rgb-1.1.1/rgb.txt
```

The release is signed by Alan Coopersmith's key, `3AB2 8523 2C46 AE43 D8E1 92F4 DAB0 F78E A6E7 E2D2`. The tarball's SHA-256 is `c80ff280a02f56c30fadc2dfa210fc6979c4ab968afa315278cb97768b64ecab`. The generator refuses an `rgb.txt` whose SHA-256 differs from its pin in `src/main.rs`, `f8e3a7bea17acc0b91e6285c5d32001db27d84c4461c655afdbce9dbeb4fb6f0`. Two runs on the same file write identical outputs.

| Path | What it is |
|---|---|
| `src/main.rs` | The generator: the pinned release and SHA-256, the folding of names, the table's layout, and SHA-256 itself. |
| `src/rgb-license.txt` | The release's `COPYING`, the MIT-style licences of The Open Group and Oracle, copied into `names.rs`. |
| `Cargo.toml`, `Cargo.lock` | The generator crate, its own workspace. |
| `.gitignore` | Leaves out `target`. |
