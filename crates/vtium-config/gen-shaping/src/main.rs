//! Generates vtium-config's shaping tables from the Unicode Character
//! Database. README.md says what it writes and how to run it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

const VERSION: &str = "18.0.0";

/// The UCD files read, by base name, with their SHA-256 and the path under
/// `https://www.unicode.org/Public/18.0.0/` they come from.
const FILES: [(&str, &str, &str); 9] = [
    (
        "emoji-sequences.txt",
        "emoji/",
        "1823dce71f3dd9cb0ad1976797baee4ffbdc1909756f43af8ce4c57f732843a4",
    ),
    (
        "emoji-zwj-sequences.txt",
        "emoji/",
        "f61b5213bdf85a57741a9f903cb38d0f39a3d13be6ff4aca3b39857e859a9191",
    ),
    (
        "ArabicShaping.txt",
        "ucd/",
        "8ccde4ebd070500a68e8bcb9d5514522fb0bebe57b9ebad3ff925b8e1e669f83",
    ),
    (
        "UnicodeData.txt",
        "ucd/",
        "0736451de439ae7baf1425136617da495e09ee5afbe6e394374db7009ea08950",
    ),
    (
        "Scripts.txt",
        "ucd/",
        "0071fd81b6aeae25f6e8bce8efec3066a6476a91b49bdb2f52dc76e817862a6a",
    ),
    (
        "emoji-data.txt",
        "ucd/emoji/",
        "80d00f8e616a0ef27fd6b8de3b758c06383b5d917e2977709578e68baf733bf1",
    ),
    (
        "IndicSyllabicCategory.txt",
        "ucd/",
        "a2b3aacf6b3e7bad4ca351ef985d9543825e20280ff280c25f646d9bc4ce304c",
    ),
    (
        "ScriptExtensions.txt",
        "ucd/",
        "5c9d34a922f687726f2a8bcf57d49f905987e51f1b21b58c95a00fbe255cec23",
    ),
    (
        "PropertyValueAliases.txt",
        "ucd/",
        "06c4c8eaf7b0bf34abe73b113da1215bd784ac254d4c223600b90267caa4bbbd",
    ),
];

/// The licence of the UCD files, copied into the output.
const LICENSE: &str = include_str!("unicode-license.txt");
/// CLDR's licence, from its release's `LICENSE`, copied into the letters
/// file: its data is under the Unicode License v3.
const CLDR_LICENSE: &str = include_str!("cldr-license.txt");

/// The scripts HarfRust 0.12 shapes with its Arabic shaper, whose joining
/// letters `JOINING` lists. No code reads `JOINING`: nothing builds the
/// joined forms ahead of time, and a letter's joined form waits on
/// shaping across cells.
const JOINING_SCRIPTS: [&str; 2] = ["Arabic", "Syriac"];

/// The scripts shaped on demand: those HarfRust 0.12 shapes with its Indic,
/// Khmer, Myanmar and Universal Shaping Engine shapers, whose syllables
/// are open sets no table lists.
const ON_DEMAND_SCRIPTS: [&str; 94] = [
    "Bengali",
    "Devanagari",
    "Gujarati",
    "Gurmukhi",
    "Kannada",
    "Malayalam",
    "Oriya",
    "Tamil",
    "Telugu",
    "Khmer",
    "Myanmar",
    "Tibetan",
    "Mongolian",
    "Sinhala",
    "Buhid",
    "Hanunoo",
    "Tagalog",
    "Tagbanwa",
    "Limbu",
    "Tai_Le",
    "Buginese",
    "Kharoshthi",
    "Syloti_Nagri",
    "Tifinagh",
    "Balinese",
    "Nko",
    "Phags_Pa",
    "Cham",
    "Kayah_Li",
    "Lepcha",
    "Rejang",
    "Saurashtra",
    "Sundanese",
    "Egyptian_Hieroglyphs",
    "Javanese",
    "Kaithi",
    "Meetei_Mayek",
    "Tai_Tham",
    "Tai_Viet",
    "Batak",
    "Brahmi",
    "Mandaic",
    "Chakma",
    "Miao",
    "Sharada",
    "Takri",
    "Duployan",
    "Grantha",
    "Khojki",
    "Khudawadi",
    "Mahajani",
    "Manichaean",
    "Modi",
    "Pahawh_Hmong",
    "Psalter_Pahlavi",
    "Siddham",
    "Tirhuta",
    "Ahom",
    "Multani",
    "Adlam",
    "Bhaiksuki",
    "Marchen",
    "Newa",
    "Masaram_Gondi",
    "Soyombo",
    "Zanabazar_Square",
    "Dogra",
    "Gunjala_Gondi",
    "Hanifi_Rohingya",
    "Makasar",
    "Medefaidrin",
    "Old_Sogdian",
    "Sogdian",
    "Elymaic",
    "Nandinagari",
    "Nyiakeng_Puachue_Hmong",
    "Wancho",
    "Chorasmian",
    "Dives_Akuru",
    "Khitan_Small_Script",
    "Yezidi",
    "Cypro_Minoan",
    "Old_Uyghur",
    "Tangsa",
    "Toto",
    "Vithkuqi",
    "Kawi",
    "Nag_Mundari",
    "Garay",
    "Gurung_Khema",
    "Kirat_Rai",
    "Ol_Onal",
    "Sunuwar",
    "Todhri",
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [ucd, cldr] = args.as_slice() else {
        eprintln!("usage: gen-shaping UCD_DIR CLDR_DIR");
        return ExitCode::from(2);
    };
    match run(Path::new(ucd), Path::new(cldr)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gen-shaping: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(ucd: &Path, cldr: &Path) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |name: &str| read_pinned(ucd, name);
    let mut emoji = Vec::new();
    for name in ["emoji-sequences.txt", "emoji-zwj-sequences.txt"] {
        emoji.extend(sequences(&read(name)?)?);
    }
    emoji.sort();
    emoji.dedup();
    let scripts = scripts(&read("Scripts.txt")?)?;
    let shaping = read("ArabicShaping.txt")?;
    let joining = joining(&shaping, &scripts)?;
    let joins = joins(&shaping)?;
    let unicode_data = read("UnicodeData.txt")?;
    let decomposed = decomposed(&unicode_data)?;
    let marks = marks(&unicode_data)?;
    let on_demand = on_demand(&scripts);
    let presentation = presentation(&read("emoji-data.txt")?)?;
    let path = root.join("src/shaping/data.rs");
    let text = data_rs(
        &emoji,
        &presentation,
        &joining,
        &joins,
        &marks,
        &decomposed,
        &on_demand,
    );
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut exemplars = Vec::new();
    for &(language, pin) in CLDR_FILES {
        let text = read_cldr(cldr, language, pin)?;
        exemplars.push((language, exemplar_ranges(&text).map_err(|e| format!("{language}.xml: {e}"))?));
    }
    let syllabic = syllabic(
        &read("IndicSyllabicCategory.txt")?,
        &read("ScriptExtensions.txt")?,
        &read("PropertyValueAliases.txt")?,
        &scripts,
    )?;
    let syllable_path = root.join("../vtium-font/src/syllable/data.rs");
    std::fs::write(&syllable_path, syllable_rs(&syllabic))
        .map_err(|e| format!("{}: {e}", syllable_path.display()))?;
    println!(
        "../vtium-font/src/syllable/data.rs: Unicode {VERSION}, {} ranges of linkers and followers, {} sets of scripts",
        syllabic.ranges.len(),
        syllabic.sets.len()
    );
    let letters_path = root.join("../vtium-font/src/letters/data.rs");
    std::fs::write(&letters_path, letters_rs(&exemplars))
        .map_err(|e| format!("{}: {e}", letters_path.display()))?;
    println!(
        "../vtium-font/src/letters/data.rs: CLDR {CLDR_VERSION}, the main exemplar characters of {} languages",
        exemplars.len()
    );
    println!(
        "src/shaping/data.rs: Unicode {VERSION}, {} emoji sequences, {} joining letters, \
         {} ranges of characters that join, {} ranges of marks, {} decomposed keys, \
         {} scripts shaped on demand, {} ranges of Emoji_Presentation",
        emoji.len(),
        joining.len(),
        joins.len(),
        marks.len(),
        decomposed.len(),
        on_demand.len(),
        presentation.len()
    );
    Ok(())
}

/// A pinned file's text, once its SHA-256 matches the pin.
fn read_pinned(ucd: &Path, name: &str) -> Result<String, String> {
    let (_, dir, pin) = FILES
        .iter()
        .find(|(n, _, _)| *n == name)
        .ok_or_else(|| format!("{name}: not a pinned file"))?;
    let path = ucd.join(name);
    let bytes = std::fs::read(&path).map_err(|e| {
        format!(
            "{}: {e}; fetch it from https://www.unicode.org/Public/{VERSION}/{dir}{name}",
            path.display()
        )
    })?;
    let got = hex(&sha256(&bytes));
    if got != *pin {
        return Err(format!("{name}: sha256 {got}, pinned {pin}"));
    }
    String::from_utf8(bytes).map_err(|e| format!("{name}: {e}"))
}

/// Each data line's fields, comments and blank lines dropped.
fn fields(text: &str) -> impl Iterator<Item = Vec<&str>> {
    text.lines().filter_map(|line| {
        let data = line.split('#').next().unwrap_or("").trim();
        (!data.is_empty()).then(|| data.split(';').map(str::trim).collect())
    })
}

fn codepoint(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s, 16).map_err(|e| format!("{s}: {e}"))
}

/// A field of one codepoint or a range `A..B`, as its first and last.
fn range(s: &str) -> Result<(u32, u32), String> {
    match s.split_once("..") {
        Some((a, b)) => Ok((codepoint(a)?, codepoint(b)?)),
        None => codepoint(s).map(|c| (c, c)),
    }
}

/// The RGI emoji sequences of more than one scalar value: every type but
/// `Basic_Emoji`.
fn sequences(text: &str) -> Result<Vec<Vec<u32>>, String> {
    let mut out = Vec::new();
    for f in fields(text) {
        let (Some(cps), Some(kind)) = (f.first(), f.get(1)) else {
            return Err(format!("a line of fewer than two fields: {f:?}"));
        };
        if *kind == "Basic_Emoji" {
            continue;
        }
        let seq = cps
            .split_whitespace()
            .map(codepoint)
            .collect::<Result<Vec<_>, _>>()?;
        if seq.len() < 2 {
            return Err(format!("a {kind} of one scalar value: {cps}"));
        }
        out.push(seq);
    }
    Ok(out)
}

/// The characters of Emoji_Presentation=Yes (emoji-data.txt), as sorted
/// ranges with each run of neighbours merged: those UTS #51 shows as emoji
/// when no variation selector follows them.
fn presentation(text: &str) -> Result<Vec<(u32, u32)>, String> {
    let mut ranges = Vec::new();
    for f in fields(text) {
        let (Some(cps), Some(property)) = (f.first(), f.get(1)) else {
            return Err(format!("a line of fewer than two fields: {f:?}"));
        };
        if *property == "Emoji_Presentation" {
            ranges.push(range(cps)?);
        }
    }
    ranges.sort_unstable();
    let mut out: Vec<(u32, u32)> = Vec::new();
    for (a, b) in ranges {
        match out.last_mut() {
            Some((_, end)) if *end + 1 >= a => *end = (*end).max(b),
            _ => out.push((a, b)),
        }
    }
    Ok(out)
}

/// Each codepoint's script, by its Scripts.txt name.
fn scripts(text: &str) -> Result<BTreeMap<u32, String>, String> {
    let mut out = BTreeMap::new();
    for f in fields(text) {
        let (Some(r), Some(name)) = (f.first(), f.get(1)) else {
            return Err(format!("a line of fewer than two fields: {f:?}"));
        };
        let (a, b) = range(r)?;
        for c in a..=b {
            out.insert(c, (*name).to_owned());
        }
    }
    Ok(out)
}

/// The joining letters of [`JOINING_SCRIPTS`]: each dual-joining (`D`) or
/// right-joining (`R`) letter, with 1 for the LAM joining group and 2 for
/// ALEF.
fn joining(text: &str, scripts: &BTreeMap<u32, String>) -> Result<Vec<(u32, char, u8)>, String> {
    let mut out = Vec::new();
    for f in fields(text) {
        let (Some(cp), Some(kind), Some(group)) = (f.first(), f.get(2), f.get(3)) else {
            return Err(format!("a line of fewer than four fields: {f:?}"));
        };
        let c = codepoint(cp)?;
        let script = scripts.get(&c).map_or("", String::as_str);
        if !JOINING_SCRIPTS.contains(&script) || !matches!(*kind, "D" | "R") {
            continue;
        }
        let group = match *group {
            "LAM" => 1,
            "ALEF" => 2,
            _ => 0,
        };
        out.push((c, if *kind == "D" { 'D' } else { 'R' }, group));
    }
    Ok(out)
}

/// The characters that join to their neighbours in every script
/// ArabicShaping.txt lists, as ranges: those of Joining_Type D (dual), R
/// (right), L (left) or C (join-causing, as the tatweel), the letters
/// ADR-1302 D2 stretches to their cells. Characters it does not list are
/// non-joining or transparent.
fn joins(text: &str) -> Result<Vec<(u32, u32)>, String> {
    let mut cps = Vec::new();
    for f in fields(text) {
        let (Some(cp), Some(kind)) = (f.first(), f.get(2)) else {
            return Err(format!("a line of fewer than three fields: {f:?}"));
        };
        if matches!(*kind, "D" | "R" | "L" | "C") {
            cps.push(codepoint(cp)?);
        }
    }
    cps.sort_unstable();
    cps.dedup();
    let mut out: Vec<(u32, u32)> = Vec::new();
    for c in cps {
        match out.last_mut() {
            Some((_, b)) if *b + 1 == c => *b = c,
            _ => out.push((c, c)),
        }
    }
    Ok(out)
}

/// Where a mark goes on its base, by its canonical combining class: 1 above
/// (classes 214, 216, 228, 230, 232 and 234), 2 below (202, 218, 220, 222
/// and 233), else 0.
fn side(ccc: u32) -> u8 {
    match ccc {
        214 | 216 | 228 | 230 | 232 | 234 => 1,
        202 | 218 | 220 | 222 | 233 => 2,
        _ => 0,
    }
}

/// The marks, as ranges of one side each: the characters of
/// General_Category Mn or Me, or of a combining class other than 0
/// (UnicodeData.txt), each with [`side`]'s value for its class.
fn marks(text: &str) -> Result<Vec<(u32, u32, u8)>, String> {
    let mut out: Vec<(u32, u32, u8)> = Vec::new();
    for f in fields(text) {
        let (Some(cp), Some(gc), Some(ccc)) = (f.first(), f.get(2), f.get(3)) else {
            return Err(format!("a line of fewer than four fields: {f:?}"));
        };
        let c = codepoint(cp)?;
        let ccc: u32 = ccc
            .parse()
            .map_err(|_| format!("{cp}: a combining class that is not a number"))?;
        if !matches!(*gc, "Mn" | "Me") && ccc == 0 {
            continue;
        }
        let s = side(ccc);
        match out.last_mut() {
            Some((_, b, last)) if *b + 1 == c && *last == s => *b = c,
            _ => out.push((c, c, s)),
        }
    }
    Ok(out)
}

/// The canonical decompositions of two or more scalar values, each key
/// the precomposed character decomposed one step at a time from its first
/// scalar: `U+1EBF` gives `U+00EA U+0301` and `U+0065 U+0302 U+0301`.
/// Hangul syllables decompose by rule, not in UnicodeData.txt, and are
/// left out.
fn decomposed(text: &str) -> Result<Vec<Vec<u32>>, String> {
    let mut map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for f in fields(text) {
        let (Some(cp), Some(dec)) = (f.first(), f.get(5)) else {
            return Err(format!("a line of fewer than six fields: {f:?}"));
        };
        if dec.is_empty() || dec.starts_with('<') {
            continue;
        }
        let d = dec
            .split_whitespace()
            .map(codepoint)
            .collect::<Result<Vec<_>, _>>()?;
        if d.len() >= 2 {
            map.insert(codepoint(cp)?, d);
        }
    }
    let mut keys = BTreeSet::new();
    for d in map.values() {
        let mut key = d.clone();
        keys.insert(key.clone());
        while let Some(more) = key.first().and_then(|c| map.get(c)) {
            key.splice(0..1, more.iter().copied());
            keys.insert(key.clone());
        }
    }
    Ok(keys.into_iter().collect())
}

/// The CLDR release whose locale data `vtium check-config` reads.
const CLDR_VERSION: &str = "49";

/// The locale file of each language in vtium-font's table of locales
/// (ADR-1401 D7), `common/main/<language>.xml` of CLDR's
/// `cldr-common-49.zip` (https://unicode.org/Public/cldr/49/), with its
/// SHA-256. Norwegian Bokmål takes `no.xml`, the parent locale that holds
/// its exemplar characters; CLDR has no file for Dhivehi, so the table
/// names none.
const CLDR_FILES: &[(&str, &str)] = &[
    ("af", "6cb39b7b008b664352d239458834e94ac49fe6d05186e80e938db8180b1a088e"),
    ("am", "7e0e10becd72f45b134ccdba7619d9f098dbffb91256fbb85524c9c28f21f26d"),
    ("ar", "c5bdda5bc6e1540091da9ab5952e88c99ed9d0110c9d3620f5813192b37a0291"),
    ("as", "d7f1bc6e318005784f748132ef389a418c46cdd8df2e4376ebc02963fdbe724e"),
    ("az", "90b3628974bf8374bee7c8b8505652aa5fadeb61498b3f0bcfbb0be4567d992d"),
    ("be", "cf521225786a3eba02fc05903d5dca2ac4c281d596699637c981f190b0bc6532"),
    ("bg", "536505cb184d7ab6f2f0236a9d23e3560379dd8d9c2f121b145f67b64fe319d8"),
    ("bn", "84b7ad61fdfbd9ed72c249fb4cef7de115fe1cd7afd33d0a40a984e8e231bdff"),
    ("bo", "74042a1f06f6d06cbbe7d85e370889d0b38aae20a6959412cf7cf3f647c1ff1c"),
    ("ca", "8c732216c1ec15e44d74104f5b517266ce170244907f75e58a0c76a945fdcb1b"),
    ("cs", "3e18918d66786f08df4432d87c364c27fb08e94ca3c1d6acc2a7e50fbe202390"),
    ("cy", "1ebb24385fc8db334f013f51941fb998a4a29c024342cde077c76ff85a1f9d0c"),
    ("da", "463f1074008731e5f1e9b178f89eb89bcc336e4b96acc433d375aa59de177c98"),
    ("de", "044e2f18f7da02aa3702b2d7357a975f5552ba0a2b204b4c05896160ad456785"),
    ("el", "1541a1e7bf92ede6dd69d0cb7614e3fba67b6862730dbe7062ddda1847a22231"),
    ("en", "236d562bcbc207efd012cf721b67ed53b711561f23457656f5d2f60a6e9a32e2"),
    ("eo", "9aad112eb73e6af614b3e7d5f3562a4947a10429dbc0165e817d1fa1d68bebd7"),
    ("es", "277d2891a2bd7206feb7a66b8419af723e6fcba38aa50cfde586cb41001d0d35"),
    ("et", "6ba3e30dc8b3a25b5d73899907f4ee046bc24e2a01bf1b78dc42060a438de0aa"),
    ("eu", "1bab95c0b50d259e42892bbe6922d32cab818fb6f42335e6c7a585695f970b45"),
    ("fa", "cd486f03425d3747000d42e766fca224509d593a62d588085fe9124341d4b6dd"),
    ("fi", "33299b06d3f963d32ae70553d7ac79f7894e3e5378db8f036742237ac51bb8f9"),
    ("fr", "e68f020eef0f6edfaf6dc13d2b7b0657c769147fd8a6ca0d4e894e0d07c026a9"),
    ("ga", "27b92f00faa538f2c8cf0d8c587530facda2b5e8098d8c561661a0db20833415"),
    ("gl", "44a8b3e78e8391e1c3621c84a555500feb269506947046ef011907e344a8b142"),
    ("gu", "a8a691d767ddbeab6efffd51130cb64ca8043830297aa4357c7f674d5fa6c7b0"),
    ("he", "1c93ae0aab935f808e9825bd69dbac67341018210dc1a5ab554d9ce49f78c038"),
    ("hi", "82a405352172aee9144b3ce57efa709bf1dfec3002892db36a320e2fb10d05e0"),
    ("hr", "d281bc43fb7847a7a573a92224ab45c61b908bf72012483c1c0d893d47505a82"),
    ("hu", "47c36143d654b89fe3ec82ef8274b65390dcc17fe9a05916967aae6ffdd3fa5e"),
    ("hy", "3e6a767549079b06fb15205c1f9cf9cd127ddb444e12bf9f63e802134c2c8587"),
    ("id", "b306620c9d10192913f60dd2285a023ccab6bbe682d3e12710b9e3fa527698cd"),
    ("is", "2de57354a42afbd0746c455d91325a2e8e83c5edc621f3d3395c9f3c4b25e1a8"),
    ("it", "ce23b3ad04c70a6d2c3917f6f9140bd657bdbcf9a127f929b3f28ea5199d7968"),
    ("ja", "14e41d40cd7422a4440139a24ed77e1e6907933e373a88e1e9e2df7bb812ac03"),
    ("ka", "8233b8563e69ccc0637a99a38aff3d7ed579a41f8eb9aef4ced284533ffdf264"),
    ("kk", "a0db5f93f07b9dbd533c418e279f886c034c9ed14d51b7f31157b86f608fc818"),
    ("km", "278952d66088a7cfa1a7ed3b589f367c55e12f249ea313383577ca7c4c1f748d"),
    ("kn", "963de0823b7cd1f61e548f4a17f3701c56a680a1647dac25f9ff10965a3e700d"),
    ("ko", "8d1286675876849101af5d207b0226ea01fef945e8017813f9828b955015f0a6"),
    ("ky", "3f7ba17e3336d1ee1d6a2c5468da49f64834f04a3eec98d91c10bef2a9f1c8bc"),
    ("lo", "99eeaf44812bb067376d5029def4731f112cae4f6e059736659a0d39e4ffc19d"),
    ("lt", "fb5aa090d5f27b51803685b9cbcdba3ec256def66513709861dd54ff2b0f462e"),
    ("lv", "47c29e963f458021011c94a3d2d49ca0921a2728d647d752c3e6a017d75e3d83"),
    ("mk", "e398e67c35b9df7cf1ebb50a81f409ed43a68fb9240257e5a9b0f05f765c16e2"),
    ("ml", "3a8cf6bdc0a22300d32a6f5c2397a6c9caf4166c7ca18150d982cf929dd70aad"),
    ("mn", "99589d447b392431f74f5c03c0db4a38688e3a52b92665ae9e9603ad3119fe0d"),
    ("mr", "62ab3bf77e3d2444b4ee9e9b4f9b9f95a14bf55b94767f04b40c6580cb9d6c63"),
    ("ms", "2317b7778f49e4ae5c12ce7ede6deb80dbe9b5bc43f87033a10d6164155757ee"),
    ("my", "6c560676474adc003a6be76b23b1ce1b62c53ddc7867181c08512856e51b0615"),
    ("no", "7f0d91082d56e7cb9aa3fa9388eea61490b929886d4506781147ccab9fb09463"),
    ("ne", "4b74f42070364b4fb26c12f29b90a87111555d8c292e4e5bbf5085629bae53d0"),
    ("nl", "66cef32af2f308bbf79eaeee1871bff82cc17c7a8ba8ab623e87cff1ef1d5d0e"),
    ("nn", "3e500342fe04d692856aaebb3ea19d9d879cfa0cb370f64f3f995ccd371c8625"),
    ("or", "3ff03c70a2469126b60cb78024e0e641654e07e421332d6cab5d157a3c626d78"),
    ("pa", "57517ab1302755f59166d07f12cb2af367ceb53fd399f35ce59c80ba6625c449"),
    ("pl", "d5185f2a442d855b78f2837eebcb9aec4bbb440ab27c276a38d3c30f304c7bac"),
    ("ps", "dd8fd2a5e7c39e241578376c9a229a6775a2d0f7421464a4535de3caa002afc5"),
    ("pt", "91dbf92a2abe67ba2b61b151d68158a0e059b99e7d73029fbef3ed4b1d7751b1"),
    ("ro", "ee35d283dde660d29ced30690befe0aae28e1b38fbce76ed59f4aadc7d4f24bc"),
    ("ru", "d9acfc5c9f30f6342f7027cd0f1803cebd2252313e794e74d23bfabbd4781145"),
    ("sa", "187d52b4a6e3b6977e6c42ac223b31ad369e9e6e035a456391740be01202a5c7"),
    ("si", "5b105743485550ddebdcd6db50cdce94b753576746882c724c985784df861b3d"),
    ("sk", "0dce72847df3726d8007c43884c63966d5ff2e022ec5f87a7ce3b3a983ee9159"),
    ("sl", "8c912f4633e41e5d65a9c8fa1e7f617602f4dc53461e3d512165d0e1ba84eeee"),
    ("sq", "7f0c59383cd7feb90cb74ad3ff549e815856829557f414b4499a03d33d19bd36"),
    ("sr", "41357b54d1755f3c64808f158934bd88c46aa142a3e0ac52c5652b7c4e965736"),
    ("sv", "8d5812e3721000ce6a40585f443243ff7ec7618f8caef0d3462c2790f84f503e"),
    ("sw", "b645ecb31d459f6773d34f6cb376584d38780ace0bf630fad54bc5c999d57b06"),
    ("ta", "256847709f3e47d3b2c827308e7c292e36e362b5b3b6eb1b17b745e59afe40de"),
    ("te", "3888d6605fe4da35dec85318dd8342614fb008bfb768bd4bea082a0c752bffaa"),
    ("tg", "954c05bf1d2b6fe8c02f6d15a5e7c1b2ae4aea8a3c584768ee5e0b2878c6f05c"),
    ("th", "d34cbf25d13b9a7624c07e28fe38a623a813c2de870d2bb225163163631d1dbd"),
    ("ti", "941a47132eb589131b6a9430d50d41db1b99b01b53286b87723756343c02765e"),
    ("tr", "dd5ff1360718392e08419a930606c724a30e07009667a98d3d1823fb0b8ed7c7"),
    ("uk", "0a4d77c6767442e058cf9bcc8b55cf30471c6e3624520c2743a1a44e450c986d"),
    ("ur", "2748477b4c5b93fcd52ee79ec5841c784fb3abeca3695fae50b469bd812dfa32"),
    ("uz", "ff708850e8bbf081ce0b862552fcad1f6e84af0026cce0f9496cef3cca5c1fd7"),
    ("vi", "f41e5761bf9d4265255f92195dbdf7e12368500a8f3421c7417a281e799aa5cf"),
    ("yi", "fc658bef693078ccdb798a8a9a95811fd398e0bce8f49b552b4df7da6ce550a4"),
    ("zh", "1fb64d2b549ac1ae17efd0479d7678951c7ca8d03ccb6d86da27e76f1a745851"),
];

/// A pinned CLDR locale file's text, once its SHA-256 matches the pin.
fn read_cldr(cldr: &Path, language: &str, pin: &str) -> Result<String, String> {
    let path = cldr.join("common/main").join(format!("{language}.xml"));
    let bytes = std::fs::read(&path).map_err(|e| {
        format!(
            "{}: {e}; fetch cldr-common-{CLDR_VERSION}.zip from https://unicode.org/Public/cldr/{CLDR_VERSION}/ and unpack it",
            path.display()
        )
    })?;
    let got = hex(&sha256(&bytes));
    if got != pin {
        return Err(format!("{language}.xml: sha256 {got}, pinned {pin}"));
    }
    String::from_utf8(bytes).map_err(|e| format!("{language}.xml: {e}"))
}

/// The characters of a locale file's main exemplar set, the
/// `<exemplarCharacters>` element with no `type`, as sorted ranges. The
/// set is written in UnicodeSet syntax: characters, `a-z` ranges,
/// `{ch}` strings, whose characters each count, and `\uXXXX`, `\UXXXXXXXX`
/// and `\x` escapes.
fn exemplar_ranges(text: &str) -> Result<Vec<(u32, u32)>, String> {
    const OPEN: &str = "<exemplarCharacters>";
    let start = text.find(OPEN).ok_or("no main exemplar set")? + OPEN.len();
    let len = text[start..].find("</exemplarCharacters>").ok_or("an exemplar set left open")?;
    let set = text[start..start + len]
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'");
    let inner = set
        .trim()
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or("an exemplar set not in brackets")?;
    let mut chars: Vec<u32> = Vec::new();
    let mut it = inner.chars().peekable();
    let mut last: Option<u32> = None;
    let mut range = false;
    while let Some(c) = it.next() {
        let one = match c {
            c if c.is_whitespace() => continue,
            '{' | '}' => {
                last = None;
                continue;
            }
            '-' if last.is_some() => {
                range = true;
                continue;
            }
            '\\' => match it.next().ok_or("an escape at the end")? {
                'u' => hex_escape(&mut it, 4)?,
                'U' => hex_escape(&mut it, 8)?,
                e => u32::from(e),
            },
            c => u32::from(c),
        };
        match (range, last) {
            (true, Some(a)) => {
                chars.extend(a + 1..=one);
                range = false;
            }
            _ => chars.push(one),
        }
        last = Some(one);
    }
    chars.sort_unstable();
    chars.dedup();
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for c in chars {
        match ranges.last_mut() {
            Some(r) if r.1 + 1 == c => r.1 = c,
            _ => ranges.push((c, c)),
        }
    }
    Ok(ranges)
}

/// `n` hex digits from `it`, as a codepoint.
fn hex_escape(it: &mut impl Iterator<Item = char>, n: usize) -> Result<u32, String> {
    let digits: String = it.take(n).collect();
    u32::from_str_radix(&digits, 16).map_err(|e| format!("\\u{digits}: {e}"))
}

/// vtium-font's `src/letters/data.rs`: each language of [`CLDR_FILES`]
/// and its main exemplar characters, as sorted ranges.
fn letters_rs(exemplars: &[(&str, Vec<(u32, u32)>)]) -> String {
    let mut out = format!(
        "// Generated by crates/vtium-config/gen-shaping from the Unicode Common\n\
         // Locale Data Repository (CLDR) {CLDR_VERSION}. Do not edit; rerun the generator.\n//\n\
         // The Unicode data it comes from is under this licence:\n//\n{}\n",
        commented(CLDR_LICENSE)
    );
    out.push_str(
        "/// Each language, by its ISO 639 code, and its main exemplar characters\n\
         /// in CLDR, the letters its everyday text uses, as sorted ranges\n\
         /// (ADR-1401 D7).\n\
         pub const EXEMPLARS: &[(&str, &[(u32, u32)])] = &[\n",
    );
    for (name, ranges) in exemplars {
        let items: Vec<String> = ranges
            .iter()
            .map(|(a, b)| format!("(0x{a:x}, 0x{b:x})"))
            .collect();
        let _ = writeln!(out, "    (\"{name}\", &[{}]),", items.join(", "));
    }
    out.push_str("];\n");
    out
}

/// Each script of [`ON_DEMAND_SCRIPTS`] and its ranges, in that order.
fn on_demand(scripts: &BTreeMap<u32, String>) -> Vec<(&'static str, Vec<(u32, u32)>)> {
    scripts_ranges(scripts, &ON_DEMAND_SCRIPTS)
}

/// Each script of `names` and its ranges in `scripts`, in that order.
fn scripts_ranges(
    scripts: &BTreeMap<u32, String>,
    names: &[&'static str],
) -> Vec<(&'static str, Vec<(u32, u32)>)> {
    names
        .iter()
        .map(|&name| {
            let mut ranges: Vec<(u32, u32)> = Vec::new();
            for (&c, s) in scripts {
                if s != name {
                    continue;
                }
                match ranges.last_mut() {
                    Some(r) if r.1 + 1 == c => r.1 = c,
                    _ => ranges.push((c, c)),
                }
            }
            (name, ranges)
        })
        .collect()
}

/// The Indic_Syllabic_Category values that ADR-1203 D1 reads, by the
/// number `syllable_rs` writes for each: the linkers, Virama (1),
/// Invisible_Stacker (2) and Consonant_With_Stacker (3), and the
/// followers, Consonant (4) and Vowel_Independent (5).
const SYLLABIC_CATEGORIES: [(&str, u8); 5] = [
    ("Virama", 1),
    ("Invisible_Stacker", 2),
    ("Consonant_With_Stacker", 3),
    ("Consonant", 4),
    ("Vowel_Independent", 5),
];

/// The characters ADR-1203 D1 joins syllables by, with their scripts.
struct Syllabic {
    /// Each run of characters of one category and one set of scripts:
    /// first, last, the category's number and the set's index in `sets`.
    ranges: Vec<(u32, u32, u8, u16)>,
    /// Each set of scripts, each script by its number among the scripts
    /// the sets name, in the order of their short names, sorted.
    sets: Vec<Vec<u8>>,
}

/// The linkers and followers of ADR-1203 D1 from IndicSyllabicCategory.txt,
/// each with its Script_Extensions: the scripts ScriptExtensions.txt lists
/// for it, else its one script from Scripts.txt, by the short name
/// PropertyValueAliases.txt gives that script.
fn syllabic(
    isc: &str,
    scx: &str,
    aliases: &str,
    scripts: &BTreeMap<u32, String>,
) -> Result<Syllabic, String> {
    let mut short: BTreeMap<&str, &str> = BTreeMap::new();
    for f in fields(aliases) {
        if let (Some(&"sc"), Some(abbr), Some(long)) = (f.first(), f.get(1), f.get(2)) {
            short.insert(long, abbr);
        }
    }
    let mut extensions: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for f in fields(scx) {
        let (Some(r), Some(names)) = (f.first(), f.get(1)) else {
            return Err(format!("a line of fewer than two fields: {f:?}"));
        };
        let (a, b) = range(r)?;
        let names: Vec<String> = names.split_whitespace().map(str::to_owned).collect();
        for c in a..=b {
            extensions.insert(c, names.clone());
        }
    }
    let mut chars: BTreeMap<u32, (u8, Vec<String>)> = BTreeMap::new();
    for f in fields(isc) {
        let (Some(r), Some(value)) = (f.first(), f.get(1)) else {
            return Err(format!("a line of fewer than two fields: {f:?}"));
        };
        let Some(&(_, category)) = SYLLABIC_CATEGORIES.iter().find(|(v, _)| v == value) else {
            continue;
        };
        let (a, b) = range(r)?;
        for c in a..=b {
            let set = match extensions.get(&c) {
                Some(names) => names.clone(),
                None => {
                    let long = scripts
                        .get(&c)
                        .ok_or_else(|| format!("U+{c:04X}: no script in Scripts.txt"))?;
                    let abbr = short
                        .get(long.as_str())
                        .ok_or_else(|| format!("{long}: no short name"))?;
                    vec![(*abbr).to_owned()]
                }
            };
            chars.insert(c, (category, set));
        }
    }
    let mut names: Vec<String> = chars.values().flat_map(|(_, s)| s.iter().cloned()).collect();
    names.sort();
    names.dedup();
    if names.len() > usize::from(u8::MAX) {
        return Err(format!("{} scripts, more than a u8 numbers", names.len()));
    }
    let mut sets: Vec<Vec<u8>> = Vec::new();
    let mut ranges: Vec<(u32, u32, u8, u16)> = Vec::new();
    for (c, (category, set)) in chars {
        let mut ids: Vec<u8> = set
            .iter()
            .filter_map(|n| names.iter().position(|m| m == n))
            .filter_map(|i| u8::try_from(i).ok())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let index = match sets.iter().position(|s| *s == ids) {
            Some(i) => i,
            None => {
                sets.push(ids);
                sets.len() - 1
            }
        };
        let index = u16::try_from(index).map_err(|_| "more than 65535 sets of scripts")?;
        match ranges.last_mut() {
            Some(r) if r.1 + 1 == c && r.2 == category && r.3 == index => r.1 = c,
            _ => ranges.push((c, c, category, index)),
        }
    }
    Ok(Syllabic { ranges, sets })
}

/// vtium-font's `src/syllable/data.rs`: the linkers and followers of
/// ADR-1203 D1 and the sets of scripts they belong to.
fn syllable_rs(s: &Syllabic) -> String {
    let mut out = format!(
        "// Generated by crates/vtium-config/gen-shaping from the Unicode Character\n\
         // Database {VERSION}. Do not edit; rerun the generator.\n//\n\
         // The Unicode data it comes from is under this licence:\n//\n{}\n",
        commented(LICENSE)
    );
    let _ = write!(
        out,
        "/// The version of the Unicode Character Database the tables come from.\n\
         pub const VERSION: &str = \"{VERSION}\";\n\n\
         /// The characters that link a syllable's clusters and that follow a\n\
         /// link (IndicSyllabicCategory.txt), as sorted ranges of one category\n\
         /// and one set of scripts each: the first and last character, the\n\
         /// category, 1 Virama, 2 Invisible_Stacker, 3 Consonant_With_Stacker,\n\
         /// 4 Consonant or 5 Vowel_Independent, and the index in [`SETS`] of\n\
         /// the characters' Script_Extensions (ScriptExtensions.txt, else the\n\
         /// one script Scripts.txt gives).\n\
         pub const SYLLABIC: &[(u32, u32, u8, u16)] = &[\n"
    );
    for (a, b, category, set) in &s.ranges {
        let _ = writeln!(out, "    (0x{a:x}, 0x{b:x}, {category}, {set}),");
    }
    out.push_str(
        "];\n\n/// Each set of scripts that [`SYLLABIC`] names, each script by its\n\
         /// number among the scripts these tables name, in the order of their\n\
         /// short names (PropertyValueAliases.txt), sorted.\n\
         pub const SETS: &[&[u8]] = &[\n",
    );
    for set in &s.sets {
        let items: Vec<String> = set.iter().map(u8::to_string).collect();
        let _ = writeln!(out, "    &[{}],", items.join(", "));
    }
    out.push_str("];\n");
    out
}

/// `text` as `//` comment lines.
fn commented(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        out.push_str(if line.is_empty() { "//" } else { "// " });
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn seq(s: &[u32]) -> String {
    let items: Vec<String> = s.iter().map(|c| format!("0x{c:x}")).collect();
    format!("&[{}]", items.join(", "))
}

fn data_rs(
    emoji: &[Vec<u32>],
    presentation: &[(u32, u32)],
    joining: &[(u32, char, u8)],
    joins: &[(u32, u32)],
    marks: &[(u32, u32, u8)],
    decomposed: &[Vec<u32>],
    on_demand: &[(&str, Vec<(u32, u32)>)],
) -> String {
    let mut out = format!(
        "// Generated by crates/vtium-config/gen-shaping from the Unicode Character\n\
         // Database {VERSION}. Do not edit; rerun the generator.\n//\n\
         // The Unicode data it comes from is under this licence:\n//\n{}\n",
        commented(LICENSE)
    );
    out.push_str(
        "/// The RGI emoji sequences of more than one scalar value, sorted:\n\
         /// keycaps, flags, tag sequences, modifier sequences and ZWJ sequences\n\
         /// (emoji-sequences.txt and emoji-zwj-sequences.txt).\n\
         pub const EMOJI: &[&[u32]] = &[\n",
    );
    for s in emoji {
        let _ = writeln!(out, "    {},", seq(s));
    }
    out.push_str(
        "];\n\n/// The characters of Emoji_Presentation=Yes, as sorted ranges\n\
         /// (emoji-data.txt): those UTS #51 shows as emoji when no variation\n\
         /// selector follows them. ADR-1200 D21 draws them from a color glyph\n\
         /// where a source has one.\n\
         pub const PRESENTATION: &[(u32, u32)] = &[\n",
    );
    for (a, b) in presentation {
        let _ = writeln!(out, "    (0x{a:x}, 0x{b:x}),");
    }
    out.push_str(
        "];\n\n/// The joining letters of the scripts the Arabic shaper takes, Arabic\n\
         /// and Syriac (ArabicShaping.txt): each letter, its joining type, `D`\n\
         /// for dual-joining or `R` for right-joining, and its joining group, 1\n\
         /// for LAM, 2 for ALEF, else 0. No code reads it today: joined forms\n\
         /// wait on shaping across cells.\n\
         pub const JOINING: &[(u32, char, u8)] = &[\n",
    );
    for (c, kind, group) in joining {
        let _ = writeln!(out, "    (0x{c:x}, '{kind}', {group}),");
    }
    out.push_str(
        "];\n\n/// The characters that join to their neighbours, in every script\n\
         /// ArabicShaping.txt lists, as sorted ranges: Joining_Type D, R, L or\n\
         /// C. ADR-1302 D2 stretches an outline glyph of one of them to its\n\
         /// cells.\n\
         pub const JOINS: &[(u32, u32)] = &[\n",
    );
    for (a, b) in joins {
        let _ = writeln!(out, "    (0x{a:x}, 0x{b:x}),");
    }
    out.push_str(
        "];\n\n/// The marks, as sorted ranges of one side each (UnicodeData.txt): the\n\
         /// characters of General_Category Mn or Me or of a combining class\n\
         /// other than 0, each with where it goes on its base by its class, 1\n\
         /// above, 2 below, else 0. ADR-1302 D3 fits a mark to the row.\n\
         pub const MARKS: &[(u32, u32, u8)] = &[\n",
    );
    for (a, b, side) in marks {
        let _ = writeln!(out, "    (0x{a:x}, 0x{b:x}, {side}),");
    }
    out.push_str(
        "];\n\n/// The canonical decompositions of two or more scalar values, each\n\
         /// decomposed one step at a time from its first scalar, sorted\n\
         /// (UnicodeData.txt).\n\
         pub const DECOMPOSED: &[&[u32]] = &[\n",
    );
    for s in decomposed {
        let _ = writeln!(out, "    {},", seq(s));
    }
    out.push_str(
        "];\n\n/// The scripts shaped on demand, each with its ranges (Scripts.txt):\n\
         /// those the Indic, Khmer, Myanmar and Universal Shaping Engine shapers\n\
         /// take.\n\
         pub const ON_DEMAND: &[(&str, &[(u32, u32)])] = &[\n",
    );
    for (name, ranges) in on_demand {
        let items: Vec<String> = ranges
            .iter()
            .map(|(a, b)| format!("(0x{a:x}, 0x{b:x})"))
            .collect();
        let _ = writeln!(out, "    (\"{name}\", &[{}]),", items.join(", "));
    }
    out.push_str("];\n");
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// SHA-256, FIPS 180-4.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (wi, word) in w.iter_mut().zip(block.as_chunks::<4>().0) {
            *wi = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for (k, wi) in K.iter().zip(w) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*k)
                .wrapping_add(wi);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (hi, x) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *hi = hi.wrapping_add(x);
        }
    }
    let mut out = [0u8; 32];
    for (chunk, hi) in out.as_chunks_mut::<4>().0.iter_mut().zip(h) {
        *chunk = hi.to_be_bytes();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_fips_180_examples() {
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_decomposition_is_keyed_at_each_step() {
        let text = "00EA;E;Ll;0;L;0065 0302;;;;N;;;;;\n\
                    1EBF;E;Ll;0;L;00EA 0301;;;;N;;;;;\n";
        let keys = decomposed(text).unwrap();
        assert!(keys.contains(&vec![0xea, 0x301]));
        assert!(keys.contains(&vec![0x65, 0x302, 0x301]));
        assert!(keys.contains(&vec![0x65, 0x302]));
    }
}
