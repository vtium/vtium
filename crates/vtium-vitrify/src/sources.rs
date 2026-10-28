//! Finding and reading a recipe's sources, as the invoking user, before
//! anything is parsed (ADR-1200 D1, D8, D14): kbd names resolved as
//! `setfont` resolves them, files, vconsole.conf with the kernel's command
//! line over it, the kernel's own font, and the built-in sets. Each source
//! becomes the bytes the parser is handed and the SHA-256 of each, from
//! which the build key is made.

use std::path::{Path, PathBuf};

use vtium_font::{PSF_BYTES_MAX, Sha256};
use vtium_platform::vt::font::{ConsoleFont, ConsoleTty};

use crate::child::Request;
use crate::recipe::{Origin, Source};

/// The kbd data directories the build names, in the order they are
/// searched: kbd's own, Debian's console-setup and the Red Hat family's.
pub const KBD_DIRS: [&str; 3] = ["/usr/share/kbd", "/usr/share", "/usr/lib/kbd"];

/// The suffixes `setfont` tries after a font's name, in order
/// (`src/libkfont/context.c`).
pub const FONT_SUFFIXES: [&str; 5] = ["", ".psfu", ".psf", ".cp", ".fnt"];
/// The suffixes it tries after a unimap's name.
pub const UNIMAP_SUFFIXES: [&str; 3] = ["", ".uni", ".sfm"];
/// The compressions it tries after each, in order (`kbdfile.c`).
pub const COMPRESSED: [&str; 5] = ["", ".gz", ".bz2", ".xz", ".zst"];

/// The most bytes vconsole.conf or the kernel's command line holds.
const CONFIG_BYTES_MAX: u64 = 64 * 1024;

/// What the host gives a build: where kbd's data is, where vconsole.conf
/// and the kernel's command line are, and the kernel's VT fonts.
pub struct Host<'a> {
    pub kbd_dirs: Vec<PathBuf>,
    pub vconsole_conf: PathBuf,
    pub kernel_cmdline: PathBuf,
    pub consoles: &'a dyn Consoles,
}

/// The kernel's VT fonts: [`KernelConsoles`] reads them, a test stands in.
pub trait Consoles {
    fn font(&self, vt: u16) -> Result<ConsoleFont, String>;
}

/// The kernel's VTs, through `/dev/tty<N>` and vtium-platform's ioctls.
pub struct KernelConsoles;

impl Consoles for KernelConsoles {
    fn font(&self, vt: u16) -> Result<ConsoleFont, String> {
        let path = PathBuf::from(format!("/dev/tty{vt}"));
        let tty = ConsoleTty::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        vtium_platform::vt::font::read(&tty).map_err(|e| match e {
            vtium_platform::vt::font::FontError::Graphics => {
                format!("VT {vt} is in graphics mode, as vtium's own VT is: name a VT in text mode")
            }
            e => format!("VT {vt}: {e}"),
        })
    }
}

/// A source read: what the parser is to be given, or a built-in set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Font(Request),
    Builtin(String),
}

/// A source ready to parse, and the SHA-256s of what was read for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub input: Input,
    /// The font's bytes as read; for a built-in, its name's.
    pub sha256: [u8; 32],
    /// The unimap's and def.uni's, when read.
    pub unimap_sha256: Option<[u8; 32]>,
    pub def_uni_sha256: Option<[u8; 32]>,
}

/// Reads at most `PSF_BYTES_MAX` bytes of `path`.
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !file.metadata().is_ok_and(|m| m.is_file()) {
        return Err(format!("{}: not a regular file", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{}: larger than {limit} bytes", path.display()));
    }
    Ok(bytes)
}

fn is_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file())
}

/// Finds `name` in `dir` as kbd's `findfile_in_dir` does at depth 0: a name
/// with a `/` looks in the subdirectory its first component names; any
/// other is `name` followed by the first of `suffixes` for which a file
/// exists plain or compressed, plain first.
fn find_in_dir(name: &str, dir: &Path, suffixes: &[&str]) -> Option<PathBuf> {
    if let Some((sub, rest)) = name.split_once('/') {
        let sub = dir.join(sub);
        return if sub.is_dir() {
            find_in_dir(rest, &sub, suffixes)
        } else {
            None
        };
    }
    suffixes.iter().find_map(|suffix| {
        COMPRESSED.iter().find_map(|z| {
            let path = dir.join(format!("{name}{suffix}{z}"));
            is_file(&path).then_some(path)
        })
    })
}

/// Finds a kbd font or unimap as `setfont` finds it (`kbdfile_find`): an
/// absolute path as given, then with each suffix, each plain then
/// compressed; any other name in `<data>/<sub>/` of each data directory in
/// turn. Unlike `setfont`, a relative name is never looked for in the
/// working directory, which an unattended `create` does not choose.
pub fn find(name: &str, data_dirs: &[PathBuf], sub: &str, suffixes: &[&str]) -> Option<PathBuf> {
    if name.starts_with('/') {
        return suffixes.iter().find_map(|suffix| {
            COMPRESSED.iter().find_map(|z| {
                let path = PathBuf::from(format!("{name}{suffix}{z}"));
                is_file(&path).then_some(path)
            })
        });
    }
    data_dirs
        .iter()
        .find_map(|d| find_in_dir(name, &d.join(sub), suffixes))
}

/// The values of `keys` in an environment-style file: `KEY=value` lines,
/// `#` and `;` comment lines, a value in single or double quotes taken
/// without them, a later line over an earlier one.
fn env_values(text: &str, keys: &[&str]) -> Vec<Option<String>> {
    let mut out = vec![None; keys.len()];
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if let Some(i) = keys.iter().position(|&key| key == k.trim()) {
            let v = v.trim();
            let v = ['"', '\'']
                .iter()
                .find_map(|&q| v.strip_prefix(q).and_then(|v| v.strip_suffix(q)))
                .unwrap_or(v);
            if let Some(slot) = out.get_mut(i) {
                *slot = Some(v.to_owned()).filter(|v| !v.is_empty());
            }
        }
    }
    out
}

/// `FONT=` and `FONT_UNIMAP=` as systemd-vconsole-setup reads them:
/// vconsole.conf, then the kernel command line's `vconsole.font=` and
/// `vconsole.font_unimap=` over it.
pub fn vconsole(conf: &str, cmdline: &str) -> (Option<String>, Option<String>) {
    let mut values = env_values(conf, &["FONT", "FONT_UNIMAP"]).into_iter();
    let (mut font, mut unimap) = (values.next().flatten(), values.next().flatten());
    for word in cmdline.split_whitespace() {
        if let Some(v) = word.strip_prefix("vconsole.font=") {
            font = Some(v.to_owned()).filter(|v| !v.is_empty());
        } else if let Some(v) = word.strip_prefix("vconsole.font_unimap=") {
            unimap = Some(v.to_owned()).filter(|v| !v.is_empty());
        }
    }
    (font, unimap)
}

/// A PSF2 font of the kernel's glyphs, with its map as the table (ADR-1200
/// D8): a VT's font is vitrified as a PSF with that map.
pub fn console_psf(font: &ConsoleFont) -> Vec<u8> {
    let row = font.width.div_ceil(8);
    let has_table = !font.unimap.is_empty();
    let mut out = Vec::with_capacity(32usize.saturating_add(font.data.len()));
    for v in [
        0x864a_b572u32,
        0,
        32,
        u32::from(has_table),
        font.charcount,
        row.saturating_mul(font.height),
        font.height,
        font.width,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&font.data);
    if has_table {
        let mut by_glyph: Vec<Vec<char>> = vec![Vec::new(); font.charcount as usize];
        for pair in &font.unimap {
            if let (Some(glyph), Some(c)) = (
                by_glyph.get_mut(usize::from(pair.fontpos)),
                char::from_u32(u32::from(pair.unicode)),
            ) {
                glyph.push(c);
            }
        }
        for chars in by_glyph {
            let mut buf = [0u8; 4];
            for c in chars {
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            out.push(0xff);
        }
    }
    out
}

/// The SHA-256 of a built-in set's name: its glyphs are the tool's own,
/// and change only with it.
fn builtin_hash(name: &str) -> [u8; 32] {
    Sha256::digest(format!("builtin:{name}").as_bytes())
}

/// Reads `source` for the parser.
pub fn resolve(source: &Source, host: &Host<'_>) -> Result<Resolved, String> {
    let limit = u64::from(PSF_BYTES_MAX);
    let find_font = |name: &str| {
        find(name, &host.kbd_dirs, "consolefonts", &FONT_SUFFIXES).ok_or_else(|| {
            format!(
                "no kbd font {name} in {}",
                host.kbd_dirs
                    .iter()
                    .map(|d| d.join("consolefonts").display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    };
    let read_unimap = |name: &str| -> Result<Vec<u8>, String> {
        let path = find(name, &host.kbd_dirs, "unimaps", &UNIMAP_SUFFIXES)
            .ok_or_else(|| format!("no kbd unimap {name}"))?;
        read_bounded(&path, limit)
    };
    let def_uni = || {
        find("def.uni", &host.kbd_dirs, "unimaps", &UNIMAP_SUFFIXES)
            .map(|p| read_bounded(&p, limit))
            .transpose()
    };
    let (font, unimap_name) = match &source.origin {
        Origin::Builtin(name) => {
            return Ok(Resolved {
                input: Input::Builtin(name.clone()),
                sha256: builtin_hash(name),
                unimap_sha256: None,
                def_uni_sha256: None,
            });
        }
        Origin::Kbd(name) => (
            read_bounded(&find_font(name)?, limit)?,
            source.unimap.clone(),
        ),
        Origin::File(path) => (read_bounded(path, limit)?, source.unimap.clone()),
        Origin::Vconsole => {
            let conf = match read_bounded(&host.vconsole_conf, CONFIG_BYTES_MAX) {
                Ok(b) => String::from_utf8_lossy(&b).into_owned(),
                Err(_) if !host.vconsole_conf.exists() => String::new(),
                Err(e) => return Err(e),
            };
            let cmdline = read_bounded(&host.kernel_cmdline, CONFIG_BYTES_MAX)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();
            let (font, unimap) = vconsole(&conf, &cmdline);
            let font = font.ok_or_else(|| {
                format!(
                    "{} names no FONT= and the kernel command line no vconsole.font=, \
                     so the console keeps the kernel's font: use console:VT",
                    host.vconsole_conf.display()
                )
            })?;
            (
                read_bounded(&find_font(&font)?, limit)?,
                source.unimap.clone().or(unimap),
            )
        }
        Origin::Console(vt) => (console_psf(&host.consoles.font(*vt)?), None),
    };
    let unimap = unimap_name.as_deref().map(read_unimap).transpose()?;
    let def_uni = if unimap.is_none() { def_uni()? } else { None };
    Ok(Resolved {
        sha256: Sha256::digest(&font),
        unimap_sha256: unimap.as_deref().map(Sha256::digest),
        def_uni_sha256: def_uni.as_deref().map(Sha256::digest),
        input: Input::Font(Request {
            font,
            unimap,
            def_uni,
        }),
    })
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    use std::fs;

    use vtium_platform::vt::font::UniPair;

    use super::*;

    struct Tree(PathBuf);

    impl Tree {
        fn new(test: &str) -> Tree {
            let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/tmp"))
                .join("vitrify-sources")
                .join(test);
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Tree(dir)
        }

        fn file(&self, rel: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
            path
        }
    }

    fn found(tree: &Tree, name: &str, dirs: &[&str]) -> Option<String> {
        let dirs: Vec<PathBuf> = dirs.iter().map(|d| tree.0.join(d)).collect();
        find(name, &dirs, "consolefonts", &FONT_SUFFIXES)
            .map(|p| p.strip_prefix(&tree.0).unwrap().display().to_string())
    }

    #[test]
    fn kbd_names_resolve_in_setfonts_order() {
        let t = Tree::new("order");
        t.file("a/consolefonts/ter.psf.gz", b"x");
        t.file("a/consolefonts/ter.psfu.zst", b"x");
        t.file("a/consolefonts/lat.psf", b"x");
        t.file("a/consolefonts/lat.psf.gz", b"x");
        t.file("a/consolefonts/exact", b"x");
        t.file("a/consolefonts/exact.psfu", b"x");
        t.file("b/consolefonts/ter.psfu", b"x");
        t.file("b/consolefonts/only-b.fnt.xz", b"x");
        t.file("b/consolefonts/sub/deep.cp.bz2", b"x");
        fs::create_dir_all(t.0.join("a/consolefonts/dir.psfu")).unwrap();
        // the first directory holding the name wins, then the first suffix
        // with a file, compressed or not, then plain before compressed
        assert_eq!(
            found(&t, "ter", &["a", "b"]).as_deref(),
            Some("a/consolefonts/ter.psfu.zst")
        );
        assert_eq!(
            found(&t, "ter", &["b", "a"]).as_deref(),
            Some("b/consolefonts/ter.psfu")
        );
        assert_eq!(
            found(&t, "lat", &["a"]).as_deref(),
            Some("a/consolefonts/lat.psf")
        );
        assert_eq!(
            found(&t, "exact", &["a"]).as_deref(),
            Some("a/consolefonts/exact")
        );
        assert_eq!(
            found(&t, "only-b", &["a", "b"]).as_deref(),
            Some("b/consolefonts/only-b.fnt.xz")
        );
        assert_eq!(
            found(&t, "sub/deep", &["a", "b"]).as_deref(),
            Some("b/consolefonts/sub/deep.cp.bz2")
        );
        // a directory is not a font, and a missing name finds nothing
        assert_eq!(found(&t, "dir", &["a"]), None);
        assert_eq!(found(&t, "missing", &["a", "b"]), None);
        // an absolute path takes the suffixes too
        let abs = t.0.join("a/consolefonts/ter");
        let p = find(abs.to_str().unwrap(), &[], "consolefonts", &FONT_SUFFIXES).unwrap();
        assert_eq!(p, t.0.join("a/consolefonts/ter.psfu.zst"));
    }

    #[test]
    fn a_relative_name_is_never_read_from_the_working_directory() {
        let t = Tree::new("cwd");
        // the test's working directory is the crate's, which holds Cargo.toml
        assert!(Path::new("Cargo.toml").exists());
        assert_eq!(found(&t, "Cargo.toml", &["a"]), None);
    }

    #[test]
    fn vconsole_reads_the_conf_with_the_cmdline_over_it() {
        let conf = "# comment\nKEYMAP=us\nFONT=\"ter-v16n\"\nFONT_UNIMAP='lat1u'\n";
        assert_eq!(
            vconsole(conf, "quiet\n"),
            (Some("ter-v16n".into()), Some("lat1u".into()))
        );
        assert_eq!(
            vconsole(
                conf,
                "root=/dev/sda vconsole.font=lat2-16 vconsole.font_unimap=x\n"
            ),
            (Some("lat2-16".into()), Some("x".into()))
        );
        assert_eq!(vconsole("FONT=\n", ""), (None, None));
        assert_eq!(vconsole("", "vconsole.keymap=us"), (None, None));
    }

    #[test]
    fn a_console_font_becomes_a_psf_with_its_map() {
        let font = ConsoleFont {
            width: 8,
            height: 2,
            charcount: 3,
            data: vec![1, 1, 2, 2, 3, 3],
            unimap: vec![
                UniPair {
                    unicode: 0x41,
                    fontpos: 1,
                },
                UniPair {
                    unicode: 0xc0,
                    fontpos: 1,
                },
                UniPair {
                    unicode: 0x2500,
                    fontpos: 2,
                },
                UniPair {
                    unicode: 0x42,
                    fontpos: 9,
                },
            ],
        };
        let psf = vtium_font::Psf::parse(&console_psf(&font)).expect("a PSF");
        assert_eq!((psf.glyph_count(), psf.glyph_size()), (3, (8, 2)));
        assert_eq!(psf.glyph_index('A'), Some(1));
        assert_eq!(psf.glyph_index('À'), Some(1));
        assert_eq!(psf.glyph_index('─'), Some(2));
        assert_eq!(psf.glyph_index('B'), None);
    }

    struct Graphics;

    impl Consoles for Graphics {
        fn font(&self, vt: u16) -> Result<ConsoleFont, String> {
            Err(format!("VT {vt} is in graphics mode"))
        }
    }

    fn host<'a>(t: &Tree, consoles: &'a dyn Consoles) -> Host<'a> {
        Host {
            kbd_dirs: vec![t.0.join("kbd")],
            vconsole_conf: t.0.join("vconsole.conf"),
            kernel_cmdline: t.0.join("cmdline"),
            consoles,
        }
    }

    fn source(spelling: &str, origin: Origin, unimap: Option<&str>) -> Source {
        Source {
            spelling: spelling.into(),
            origin,
            unimap: unimap.map(Into::into),
            cover: Vec::new(),
        }
    }

    #[test]
    fn each_source_reads_its_bytes_and_hashes_them() {
        let t = Tree::new("resolve");
        t.file("kbd/consolefonts/f.psfu.gz", b"font bytes");
        t.file("kbd/unimaps/lat1u.uni", b"0x41 U+0041\n");
        t.file("kbd/unimaps/def.uni", b"0x00 U+0041\n");
        let consoles = Graphics;
        let h = host(&t, &consoles);
        let r = resolve(&source("kbd:f", Origin::Kbd("f".into()), None), &h).unwrap();
        assert_eq!(r.sha256, Sha256::digest(b"font bytes"));
        assert_eq!(r.def_uni_sha256, Some(Sha256::digest(b"0x00 U+0041\n")));
        let r = resolve(&source("kbd:f", Origin::Kbd("f".into()), Some("lat1u")), &h).unwrap();
        assert_eq!(r.unimap_sha256, Some(Sha256::digest(b"0x41 U+0041\n")));
        // def.uni is not read when the recipe names a unimap
        assert_eq!(r.def_uni_sha256, None);
        let e = resolve(&source("kbd:g", Origin::Kbd("g".into()), None), &h).unwrap_err();
        assert!(e.starts_with("no kbd font g in "), "{e}");
        let e = resolve(&source("console:3", Origin::Console(3), None), &h).unwrap_err();
        assert!(e.contains("graphics mode"), "{e}");
    }

    /// A stand-in kernel holding an 8x4 font of 256 glyphs for VT 3.
    struct Kernel;

    impl Consoles for Kernel {
        fn font(&self, vt: u16) -> Result<ConsoleFont, String> {
            if vt != 3 {
                return Err(format!("VT {vt}: no such VT"));
            }
            Ok(ConsoleFont {
                width: 8,
                height: 4,
                charcount: 256,
                data: (0..256u32 * 4).map(|i| (i / 4) as u8).collect(),
                unimap: (0x20..0x7f)
                    .map(|c| UniPair {
                        unicode: c,
                        fontpos: c,
                    })
                    .chain([UniPair {
                        unicode: 0x2588,
                        fontpos: 0xdb,
                    }])
                    .collect(),
            })
        }
    }

    #[test]
    fn the_kernels_font_parses_as_a_psf_with_its_map() {
        let t = Tree::new("console");
        let consoles = Kernel;
        let h = host(&t, &consoles);
        let r = resolve(&source("console:3", Origin::Console(3), None), &h).unwrap();
        let Input::Font(request) = &r.input else {
            panic!("a font")
        };
        assert_eq!(r.sha256, Sha256::digest(&request.font));
        let g = crate::decode::font(&request.font, None, None).expect("parses");
        assert_eq!(
            (g.width, g.height, g.count, g.format.as_str()),
            (8, 4, 256, "psf2+table")
        );
        assert_eq!(g.map.len(), 0x7f - 0x20 + 1);
        assert!(g.map.contains(&(0x2588, 0xdb)));
        assert_eq!(g.glyph(0xdb), Some(&[0xdbu8; 4][..]));
        // the kernel's answer for another VT names it
        let e = resolve(&source("console:4", Origin::Console(4), None), &h).unwrap_err();
        assert_eq!(e, "VT 4: no such VT");
    }

    #[test]
    fn vconsole_resolves_its_font_as_a_kbd_name() {
        let t = Tree::new("vconsole");
        t.file("kbd/consolefonts/lat2-16.psfu.gz", b"lat2");
        t.file("kbd/consolefonts/ter-v16n.psf.gz", b"ter");
        t.file("kbd/unimaps/x.uni", b"0x41 U+0041\n");
        let consoles = Graphics;
        let h = host(&t, &consoles);
        let vc = source("vconsole", Origin::Vconsole, None);
        let e = resolve(&vc, &h).unwrap_err();
        assert!(e.contains("names no FONT="), "{e}");
        t.file("vconsole.conf", b"FONT=ter-v16n\n");
        assert_eq!(resolve(&vc, &h).unwrap().sha256, Sha256::digest(b"ter"));
        t.file("cmdline", b"vconsole.font=lat2-16 vconsole.font_unimap=x\n");
        let r = resolve(&vc, &h).unwrap();
        assert_eq!(r.sha256, Sha256::digest(b"lat2"));
        assert_eq!(r.unimap_sha256, Some(Sha256::digest(b"0x41 U+0041\n")));
    }
}
