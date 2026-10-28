//! vtium-vitrify's commands, run as the invoking user against a store in
//! a test directory (`--store`), a kbd tree and frits generated here:
//! `create --out`, `create` into the store with its rebuild rule,
//! `install`, `list`, `verify` and `remove`, and the parser's child.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;

use vtium_font::set::{Set, SetFace};
use vtium_font::{Face, Psf, Sha256, Style};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-vitrify");

/// The first ancestor of `dir` that the store's check refuses as a parent
/// for `uid`, if any.
fn refused_parent(dir: &Path, uid: u32) -> Option<String> {
    dir.ancestors().find_map(|a| match fs::symlink_metadata(a) {
        Ok(m) if m.is_symlink() => Some(format!("{} is a symbolic link", a.display())),
        Ok(m) if ![0, uid].contains(&m.uid()) || m.mode() & 0o022 != 0 => Some(format!(
            "{} is owned by uid {} with mode {:o}",
            a.display(),
            m.uid(),
            m.mode() & 0o7777
        )),
        Ok(_) => None,
        Err(e) => Some(format!("{}: {e}", a.display())),
    })
}

/// A fresh directory for `test` under the first of
/// `VTIUM_FONT_STORE_TEST_DIR`, Cargo's temporary directory and
/// `XDG_RUNTIME_DIR` whose ancestors the store's check takes as parents.
fn dir(test: &str) -> PathBuf {
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let me = fs::metadata(&tmp).expect("stat").uid();
    let candidates: Vec<PathBuf> = std::env::var_os("VTIUM_FONT_STORE_TEST_DIR")
        .map(PathBuf::from)
        .into_iter()
        .chain([tmp])
        .chain(std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from))
        .filter_map(|c| fs::canonicalize(c).ok())
        .collect();
    let base = candidates
        .iter()
        .find(|c| refused_parent(c, me).is_none())
        .unwrap_or_else(|| {
            panic!("no directory whose ancestors only root or uid {me} may write; set VTIUM_FONT_STORE_TEST_DIR")
        });
    let d = base.join("vtium-vitrify-tests").join(test);
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).expect("mkdir");
    for p in [d.as_path(), d.parent().expect("parent")] {
        fs::set_permissions(p, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    d
}

/// A PSF2 font, `width` by `height`, glyph `g` drawn as its number's bits
/// in every row, its table mapping `table`'s codepoints.
fn psf2(width: u32, height: u32, count: u32, table: Option<&[(u32, char)]>) -> Vec<u8> {
    let row = width.div_ceil(8);
    let mut f = Vec::new();
    for v in [
        0x864a_b572u32,
        0,
        32,
        u32::from(table.is_some()),
        count,
        row * height,
        height,
        width,
    ] {
        f.extend_from_slice(&v.to_le_bytes());
    }
    for g in 0..count {
        for y in 0..height {
            for b in 0..row {
                f.push(((g * 7 + y * 3 + b) % 256) as u8);
            }
        }
    }
    if let Some(table) = table {
        for g in 0..count {
            for &(tg, c) in table {
                if tg == g {
                    let mut buf = [0; 4];
                    f.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
            f.push(0xff);
        }
    }
    f
}

/// A PSF1 font of 256 8x`height` glyphs with no table.
fn psf1(height: u8) -> Vec<u8> {
    let mut f = vec![0x36, 0x04, 0, height];
    for g in 0..256u32 {
        for y in 0..u32::from(height) {
            f.push(((g + y) % 256) as u8);
        }
    }
    f
}

/// CRC-32, for the gzip trailer.
fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0u32, |crc, &b| {
        (0..8).fold(crc ^ u32::from(b), |c, _| {
            if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            }
        })
    })
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
    out.extend(miniz_oxide::deflate::compress_to_vec(bytes, 6));
    out.extend_from_slice(&crc32(bytes).to_le_bytes());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out
}

/// The table of the test fonts: ASCII letters and a few more.
fn table() -> Vec<(u32, char)> {
    let mut t: Vec<(u32, char)> = ('A'..='Z').zip(1u32..).map(|(c, g)| (g, c)).collect();
    t.push((1, 'À'));
    t.push((40, '─'));
    t.push((41, '█'));
    t
}

/// A test's directory: a kbd tree, frits, a store and a place for files.
struct World {
    root: PathBuf,
}

impl World {
    fn new(test: &str) -> World {
        let root = dir(test);
        for d in ["kbd/consolefonts", "kbd/unimaps", "frits", "out"] {
            fs::create_dir_all(root.join(d)).expect("mkdir");
        }
        fs::write(
            root.join("kbd/consolefonts/ter-v16n.psf.gz"),
            gzip(&psf2(8, 16, 64, Some(&table()))),
        )
        .expect("font");
        fs::write(
            root.join("kbd/unimaps/def.uni"),
            "0x41 U+0041\n0x42-0x43 U+0042-U+0043\n",
        )
        .expect("def.uni");
        World { root }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn store(&self) -> PathBuf {
        self.path("store")
    }

    fn frit(&self, name: &str, text: &str) -> PathBuf {
        let p = self.path(&format!("frits/{name}.vtfrit"));
        fs::write(&p, text).expect("frit");
        p
    }

    /// Runs the tool with the test's kbd tree, frits and store.
    fn run(&self, args: &[&str]) -> Output {
        Command::new(BIN)
            .arg("--kbd-dir")
            .arg(self.path("kbd"))
            .arg("--frits")
            .arg(self.path("frits"))
            .arg("--store")
            .arg(self.store())
            .arg("--vconsole-conf")
            .arg(self.path("vconsole.conf"))
            .arg("--kernel-cmdline")
            .arg(self.path("cmdline"))
            .args(args)
            .output()
            .expect("run vtium-vitrify")
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn status(o: &Output) -> i32 {
    o.status.code().expect("exited")
}

/// Every codepoint drawn from `set` at its cell equals `psf`'s own draw.
fn draws_as(set: Arc<Set>, psf: &Psf, chars: &[char]) {
    let (w, h) = psf.glyph_size();
    let face = SetFace::new(set, h).expect("a strike");
    assert_eq!((face.cell().width, face.cell().height), (w, h));
    for &c in chars {
        let ours = face
            .draw_unscaled(&[c], Style::default())
            .unwrap_or_else(|| panic!("{c:?} is in the set"));
        let theirs = psf
            .render(psf.glyph_index(c).expect("in the font"), Style::default())
            .expect("draws");
        assert_eq!(ours.pixels(), theirs.pixels(), "{c:?}");
    }
}

#[test]
fn create_out_builds_a_portable_set_that_draws_as_the_font() {
    let w = World::new("create-out");
    let frit = w.frit("ter", "source=kbd:ter-v16n\nlicense=OFL-1.1\n");
    let out = w.path("out/ter.vtglass");
    let o = w.run(&[
        "create",
        "--out",
        out.to_str().unwrap(),
        frit.to_str().unwrap(),
    ]);
    assert_eq!(status(&o), 0, "{}{}", stdout(&o), stderr(&o));
    assert_eq!(stdout(&o), "built\tter\n");
    let bytes = fs::read(&out).expect("the set");
    let set = Arc::new(Set::from_bytes(bytes.clone()).expect("reads"));
    set.check_chunks().expect("chunks");
    assert_eq!(set.name(), "ter");
    let meta: Vec<(String, String)> = set
        .meta()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    let get = |k: &str| meta.iter().find(|(m, _)| m == k).map(|(_, v)| v.as_str());
    assert_eq!(get("source"), Some("kbd:ter-v16n"));
    assert_eq!(get("source-format"), Some("psf2+gzip+table"));
    assert_eq!(get("frit"), Some("source=kbd:ter-v16n"));
    assert_eq!(get("license"), Some("OFL-1.1"));
    let font = fs::read(w.path("kbd/consolefonts/ter-v16n.psf.gz")).unwrap();
    assert_eq!(
        get("source-sha256"),
        Some(vtium_font::sha256_hex(&Sha256::digest(&font)).as_str())
    );
    // no host path, host name or time is written
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains(w.root.to_str().unwrap()));
    let psf = Psf::parse(&font).expect("the font");
    let chars: Vec<char> = table().iter().map(|&(_, c)| c).collect();
    draws_as(set, &psf, &chars);
    // the same frit and sources give the same bytes
    let again = w.path("out/again.vtglass");
    assert_eq!(
        status(&w.run(&[
            "create",
            "--out",
            again.to_str().unwrap(),
            frit.to_str().unwrap()
        ])),
        0
    );
    assert_eq!(fs::read(&again).unwrap(), bytes);
}

#[test]
fn a_font_without_a_table_takes_its_unimap_then_def_uni() {
    let w = World::new("unimap");
    fs::write(w.path("fonts.psf"), psf1(8)).unwrap();
    fs::write(w.path("kbd/unimaps/mine.uni"), "0x61 U+0061\n").unwrap();
    let file = format!("source=file:{}\n", w.path("fonts.psf").display());
    let out = w.path("out/a.vtglass");
    let run = |text: &str| {
        let r = w.frit("a", text);
        let o = w.run(&[
            "create",
            "--out",
            out.to_str().unwrap(),
            r.to_str().unwrap(),
        ]);
        (status(&o), stdout(&o))
    };
    assert_eq!(run(&file).0, 0);
    let set = Arc::new(Set::from_bytes(fs::read(&out).unwrap()).unwrap());
    assert_eq!(
        set.meta_values("source-format").next(),
        Some("psf1+def.uni")
    );
    let face = SetFace::new(set, 8).unwrap();
    assert!(face.has_glyph('A') && face.has_glyph('C') && !face.has_glyph('a'));
    assert_eq!(run(&format!("{file}unimap=mine\n")).0, 0);
    let set = Arc::new(Set::from_bytes(fs::read(&out).unwrap()).unwrap());
    assert_eq!(set.meta_values("source-format").next(), Some("psf1+unimap"));
    let face = SetFace::new(set, 8).unwrap();
    assert!(face.has_glyph('a') && !face.has_glyph('A'));
    // with no def.uni, the frit must name a unimap
    fs::remove_file(w.path("kbd/unimaps/def.uni")).unwrap();
    let (code, line) = run(&file);
    assert_eq!(code, 1);
    assert!(
        line.starts_with("failed\ta\t") && line.contains("unimap="),
        "{line}"
    );
}

#[test]
fn an_absolute_path_builds_with_a_warning_that_it_is_not_portable() {
    let w = World::new("portable");
    fs::write(w.path("fonts.psf"), psf2(8, 16, 64, Some(&table()))).unwrap();
    fs::write(w.path("mine.uni"), "0x01 U+0041\n").unwrap();
    let out = w.path("out/p.vtglass");
    let run = |text: &str| {
        let r = w.frit("p", text);
        w.run(&[
            "create",
            "--out",
            out.to_str().unwrap(),
            r.to_str().unwrap(),
        ])
    };
    let o = run(&format!(
        "# a comment\nsource=file:{}\nunimap={}\n",
        w.path("fonts.psf").display(),
        w.path("mine.uni").display()
    ));
    assert_eq!((status(&o), stdout(&o)), (0, "built\tp\n".to_owned()));
    let frit = w.path("frits/p.vtfrit").display().to_string();
    assert_eq!(
        stderr(&o),
        format!(
            "vtium-vitrify: warning: {frit}:2: source=file: names a path on this machine, so the frit is not portable\n\
             vtium-vitrify: warning: {frit}:3: unimap= names a path on this machine, so the frit is not portable\n"
        )
    );
    // a kbd name is found on whatever machine builds it: no warning
    let o = run("source=kbd:ter-v16n\n");
    assert_eq!((status(&o), stderr(&o)), (0, String::new()));
    // create into the store warns too
    let o = w.run(&["create", "p"]);
    assert_eq!(status(&o), 0);
    assert_eq!(stderr(&o), "");
    w.frit(
        "q",
        &format!("source=file:{}\n", w.path("fonts.psf").display()),
    );
    let o = w.run(&["create", "q"]);
    assert_eq!(status(&o), 0);
    assert!(
        stderr(&o).contains("q.vtfrit:1: source=file: names a path on this machine"),
        "{}",
        stderr(&o)
    );
}

#[test]
fn a_kbd_font_in_each_compression_builds_the_same_glyphs() {
    let w = World::new("compressions");
    let plain = psf2(8, 16, 64, Some(&table()));
    let psf = Psf::parse(&plain).expect("the font");
    let chars: Vec<char> = table().iter().map(|&(_, c)| c).collect();
    let mut bz = Vec::new();
    bzip2::read::BzEncoder::new(plain.as_slice(), bzip2::Compression::best())
        .read_to_end(&mut bz)
        .unwrap();
    let mut xz = lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::default()).unwrap();
    xz.write_all(&plain).unwrap();
    let xz = xz.finish().unwrap();
    let zst = ruzstd::encoding::compress_to_vec(
        plain.as_slice(),
        ruzstd::encoding::CompressionLevel::Fastest,
    );
    for (ext, bytes, name) in [("bz2", bz, "bzip2"), ("xz", xz, "xz"), ("zst", zst, "zstd")] {
        let font = w.path(&format!("kbd/consolefonts/z.psf.{ext}"));
        fs::write(&font, bytes).unwrap();
        let r = w.frit("z", "source=kbd:z\n");
        let out = w.path(&format!("out/z-{ext}.vtglass"));
        let o = w.run(&[
            "create",
            "--out",
            out.to_str().unwrap(),
            r.to_str().unwrap(),
        ]);
        assert_eq!(
            (status(&o), stdout(&o)),
            (0, "built\tz\n".to_owned()),
            "{name}: {}",
            stderr(&o)
        );
        let set = Arc::new(Set::from_bytes(fs::read(&out).unwrap()).unwrap());
        assert_eq!(
            set.meta_values("source-format").next(),
            Some(format!("psf2+{name}+table").as_str())
        );
        draws_as(set, &psf, &chars);
        fs::remove_file(&font).unwrap();
    }
}

#[test]
fn install_list_verify_and_remove_keep_the_store() {
    let w = World::new("store");
    let frit = w.frit("ter", "source=kbd:ter-v16n\n");
    let file = w.path("out/ter.vtglass");
    assert_eq!(
        status(&w.run(&[
            "create",
            "--out",
            file.to_str().unwrap(),
            frit.to_str().unwrap()
        ])),
        0
    );
    let bytes = fs::read(&file).unwrap();
    let sha = vtium_font::sha256_hex(&Sha256::digest(&bytes));

    let o = w.run(&["install", file.to_str().unwrap()]);
    assert_eq!(
        (status(&o), stdout(&o)),
        (0, "installed\tter\n".to_owned()),
        "{}",
        stderr(&o)
    );
    assert_eq!(
        fs::read(w.store().join(format!("{sha}.vtglass"))).unwrap(),
        bytes
    );
    let manifest = fs::read_to_string(w.store().join("manifest")).unwrap();
    assert!(manifest.starts_with("# vtium-vitrify manifest 1\n"));
    assert!(manifest.contains(&format!("ter\t{sha}\t")), "{manifest}");
    assert!(
        manifest.ends_with("\t1.0\t8x16\tkbd:ter-v16n\n"),
        "{manifest}"
    );
    // the same set again is current
    let o = w.run(&["install", file.to_str().unwrap()]);
    assert_eq!(stdout(&o), "current\tter\n");

    let o = w.run(&["list"]);
    assert_eq!(status(&o), 0);
    let list = stdout(&o);
    let lines: Vec<&str> = list.lines().collect();
    assert_eq!(
        lines[0],
        "name\tkind\tsha256\tbuild-key\tformat\tcells\tsource"
    );
    assert!(
        lines[1].starts_with(&format!("ter\tbuilt\t{sha}\t")),
        "{list}"
    );
    assert!(
        lines[2].starts_with("unifont\tbuilt-in\t-\t-\t1.0\t8x16\tbuiltin:unifont"),
        "{list}"
    );
    assert!(
        lines[3].starts_with("unscii-8\tbuilt-in\t-\t-\t1.0\t8x8\tbuiltin:unscii-8"),
        "{list}"
    );
    let json = stdout(&w.run(&["list", "--json"]));
    assert!(
        json.starts_with(&format!(
            "[{{\"name\":\"ter\",\"kind\":\"built\",\"sha256\":\"{sha}\","
        )),
        "{json}"
    );
    assert!(json.contains("{\"name\":\"unifont\",\"kind\":\"built-in\",\"sha256\":null,\"build-key\":null,\"format\":\"1.0\",\"cells\":[\"8x16\"],\"source\":\"builtin:unifont\"}"), "{json}");

    let o = w.run(&["verify"]);
    assert_eq!((status(&o), stdout(&o)), (0, "verified\tter\n".to_owned()));
    let o = w.run(&["verify", "nope"]);
    assert_eq!(status(&o), 1);
    assert!(stdout(&o).starts_with("failed\tnope\t"));

    // a byte changed in the store's copy fails the console's check
    let stored = w.store().join(format!("{sha}.vtglass"));
    let mut bad = bytes.clone();
    let last = bad.len() - 1;
    bad[last] ^= 1;
    fs::write(&stored, &bad).unwrap();
    let o = w.run(&["verify", "ter"]);
    assert_eq!(status(&o), 1);
    assert!(stdout(&o).starts_with("failed\tter\t"), "{}", stdout(&o));
    fs::write(&stored, &bytes).unwrap();

    let o = w.run(&["remove", "ter"]);
    assert_eq!((status(&o), stdout(&o)), (0, "removed\tter\n".to_owned()));
    assert!(!stored.exists());
    assert!(!stdout(&w.run(&["list"])).contains("ter\tbuilt"));
    let o = w.run(&["remove", "ter"]);
    assert_eq!(status(&o), 1);
    // the frit stays
    assert!(frit.exists());
}

#[test]
fn install_refuses_a_changed_set() {
    let w = World::new("install-refuses");
    let frit = w.frit("ter", "source=kbd:ter-v16n\ncompress=no\n");
    let file = w.path("out/ter.vtglass");
    assert_eq!(
        status(&w.run(&[
            "create",
            "--out",
            file.to_str().unwrap(),
            frit.to_str().unwrap()
        ])),
        0
    );
    let bytes = fs::read(&file).unwrap();
    // a byte in the last glyph chunk: its own hash no longer matches
    let mut chunk = bytes.clone();
    let last = chunk.len() - 1;
    chunk[last] ^= 0x80;
    // the major version
    let mut major = bytes.clone();
    major[8] = 9;
    for (name, bad, why) in [("chunk", chunk, "sha-256"), ("major", major, "version")] {
        let p = w.path(&format!("out/{name}.vtglass"));
        fs::write(&p, bad).unwrap();
        let o = w.run(&["install", p.to_str().unwrap()]);
        assert_eq!(status(&o), 1, "{name}");
        let line = stdout(&o);
        assert!(
            line.starts_with("failed\t") && line.to_lowercase().contains(why),
            "{name}: {line}"
        );
    }
    assert!(!w.store().join("manifest").exists());
}

#[test]
fn create_builds_into_the_store_only_when_its_inputs_change() {
    let w = World::new("rebuild");
    w.frit("ter", "source=kbd:ter-v16n\n");
    let built = |o: &Output| (status(o), stdout(o));
    assert_eq!(
        built(&w.run(&["create", "ter"])),
        (0, "built\tter\n".to_owned())
    );
    assert_eq!(
        built(&w.run(&["create", "ter"])),
        (0, "current\tter\n".to_owned())
    );
    assert_eq!(
        built(&w.run(&["create", "--force", "ter"])),
        (0, "built\tter\n".to_owned())
    );
    // a changed frit rebuilds
    w.frit("ter", "source=kbd:ter-v16n\nlicense=OFL-1.1\n");
    assert_eq!(
        built(&w.run(&["create", "ter"])),
        (0, "built\tter\n".to_owned())
    );
    assert_eq!(
        built(&w.run(&["create", "ter"])),
        (0, "current\tter\n".to_owned())
    );
    // a changed source rebuilds
    fs::write(
        w.path("kbd/consolefonts/ter-v16n.psf.gz"),
        gzip(&psf2(8, 16, 64, Some(&table()[..5]))),
    )
    .unwrap();
    assert_eq!(
        built(&w.run(&["create", "ter"])),
        (0, "built\tter\n".to_owned())
    );
    let o = w.run(&["verify"]);
    assert_eq!(stdout(&o), "verified\tter\n");
    // one set file: the old one is gone
    let sets = fs::read_dir(w.store())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".vtglass")
        })
        .count();
    assert_eq!(sets, 1);
}

#[test]
fn create_all_reports_each_set_and_fails_only_the_broken_one() {
    let w = World::new("all");
    w.frit("good", "source=kbd:ter-v16n\n");
    fs::write(w.path("broken.psf"), b"\x72\xb5\x4a\x86 a header cut short").unwrap();
    w.frit(
        "broken",
        &format!("source=file:{}\n", w.path("broken.psf").display()),
    );
    w.frit("typo", "source=kbd:ter-v16n\nsize=24\n");
    let o = w.run(&["create", "--all"]);
    assert_eq!(status(&o), 1);
    let out = stdout(&o);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{out}");
    assert!(
        lines[0].starts_with("failed\tbroken\tsource=file:"),
        "{out}"
    );
    assert_eq!(lines[1], "built\tgood");
    assert!(
        lines[2].starts_with("failed\ttypo\t") && lines[2].contains(":2: size="),
        "{out}"
    );
}

#[test]
fn a_fallback_fills_what_the_first_source_lacks_on_its_baseline() {
    let w = World::new("fallback");
    w.frit(
        "mix",
        "source=kbd:ter-v16n\nsource=builtin:unscii-8\ncover=U+2580-U+259F\n",
    );
    let out = w.path("out/mix.vtglass");
    let o = w.run(&[
        "create",
        "--out",
        out.to_str().unwrap(),
        w.path("frits/mix.vtfrit").to_str().unwrap(),
    ]);
    assert_eq!(status(&o), 0, "{}", stdout(&o));
    let set = Arc::new(Set::from_bytes(fs::read(&out).unwrap()).unwrap());
    let face = SetFace::new(set, 16).unwrap();
    // U+2588 is the first source's; U+2580 comes from Unscii-8, 8x8 into
    // 8x16 at factor 1, on the bottom rows; 'a' is outside cover=
    assert!(face.has_glyph('█') && face.has_glyph('▀') && !face.has_glyph('a'));
    let unscii = vtium_font::set::builtin("unscii-8").unwrap();
    let theirs = SetFace::new(unscii, 8)
        .unwrap()
        .draw_unscaled(&['▀'], Style::default())
        .unwrap();
    let ours = face.draw_unscaled(&['▀'], Style::default()).unwrap();
    assert_eq!((ours.width(), ours.height()), (8, 16));
    assert!(
        ours.pixels()[..8 * 8].iter().all(|&p| p == 0),
        "the top half is empty"
    );
    assert_eq!(&ours.pixels()[8 * 8..], theirs.pixels());
}

#[test]
fn vconsole_builds_the_font_vconsole_conf_names() {
    let w = World::new("vconsole");
    fs::write(w.path("vconsole.conf"), "KEYMAP=us\nFONT=ter-v16n\n").unwrap();
    let o = w.run(&["create", "--vconsole"]);
    assert_eq!(
        (status(&o), stdout(&o)),
        (0, "built\tter-v16n\n".to_owned()),
        "{}",
        stderr(&o)
    );
    assert_eq!(
        fs::read_to_string(w.path("frits/ter-v16n.vtfrit")).unwrap(),
        "source=vconsole\n"
    );
    let o = w.run(&["create", "--vconsole"]);
    assert_eq!(stdout(&o), "current\tter-v16n\n");
    // a different frit of that name is not overwritten
    w.frit("ter-v16n", "source=kbd:ter-v16n\n");
    let o = w.run(&["create", "--vconsole"]);
    assert_eq!(status(&o), 1);
    assert!(
        stderr(&o).contains("holds a different frit"),
        "{}",
        stderr(&o)
    );
    // the shortcut for a kbd name writes its frit too
    let o = w.run(&["create", "--kbd", "ter-v16n"]);
    assert_eq!(
        (status(&o), stdout(&o)),
        (0, "built\tter-v16n\n".to_owned())
    );
}

#[test]
fn usage_errors_exit_2() {
    let w = World::new("usage");
    for args in [
        &["frobnicate"][..],
        &[],
        &["create"],
        &["create", "--out"],
        &["create", "--out", "x"],
        &["install"],
        &["list", "x"],
        &["--bogus", "list"],
        &["create", "--kbd", "a", "--vconsole"],
    ] {
        let o = w.run(args);
        assert_eq!(status(&o), 2, "{args:?}: {}", stderr(&o));
        assert!(stderr(&o).contains("--help"), "{args:?}");
    }
    let o = Command::new(BIN).arg("--help").output().unwrap();
    assert_eq!(status(&o), 0);
    assert!(stdout(&o).starts_with("Usage: vtium-vitrify"));
}

#[test]
fn writing_the_system_store_needs_root() {
    if rustix::process::geteuid().is_root() {
        eprintln!("skipped: run as root, this would write the system's store");
        return;
    }
    for args in [
        &["install", "x.vtglass"][..],
        &["remove", "x"],
        &["create", "--all"],
    ] {
        let o = Command::new(BIN).args(args).output().unwrap();
        assert_eq!(status(&o), 1, "{args:?}");
        assert!(
            stderr(&o).contains("sudo vtium-vitrify"),
            "{args:?}: {}",
            stderr(&o)
        );
    }
}

/// The parser, started as the tool starts it, sets no-new-privileges and
/// its limits before it reads anything.
#[test]
fn the_parser_runs_without_new_privileges_and_within_its_limits() {
    let mut child = Command::new(BIN)
        .arg("--parse")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let status_path = format!("/proc/{}/status", child.id());
    let mut ready = false;
    for _ in 0..1_000_000 {
        if fs::read_to_string(&status_path).is_ok_and(|s| {
            s.lines()
                .any(|l| l.split_whitespace().eq(["NoNewPrivs:", "1"]))
        }) {
            ready = true;
            break;
        }
    }
    let limits = fs::read_to_string(format!("/proc/{}/limits", child.id())).unwrap();
    drop(child.stdin.take());
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    let _ = child.stdout.take().unwrap().read_to_end(&mut Vec::new());
    assert!(!child.wait().unwrap().success());
    assert!(ready, "NoNewPrivs never set");
    let limit = |name: &str| {
        limits
            .lines()
            .find(|l| l.starts_with(name))
            .map(|l| {
                l[name.len()..]
                    .split_whitespace()
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    };
    assert_eq!(limit("Max address space"), "536870912 536870912");
    assert_eq!(limit("Max cpu time"), "30 30");
    assert_eq!(limit("Max processes"), "0 0");
    assert_eq!(limit("Max file size"), "0 0");
    // an empty request is refused, not parsed
    assert!(err.contains("malformed parse request"), "{err}");
    let _ = std::io::stdout().flush();
}
