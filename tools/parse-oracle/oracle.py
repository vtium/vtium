#!/usr/bin/env python3
"""The parse oracle: vtium-parse against the C reference's parser.

Extracts the C reference's parser from the libtsm commit pin.tsv names,
builds it into harness/oracle.c and builds the Rust side (src/main.rs),
then runs both over the same event scripts and compares their traces line
by line. The C side applies each departure departures.tsv lists, so a
difference no listed departure accounts for fails the run.

    oracle.py run        the differential over the corpus and the generator
    oracle.py selftest   shows the run turns red for each planted defect,
                         each departure taken out of the expectations, and
                         a moved line range

Needs git, gcc and cargo. Writes only under --out (default: out/ here).
"""

import argparse
import os
import random
import re
import shutil
import struct
import subprocess
import sys
import time
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
PARSE_README = REPO / "crates" / "vtium-parse" / "README.md"


class Refused(Exception):
    """The setup cannot go on: a moved range, a missing commit, a bad list."""


# --- the pin, the ranges and the expectations ------------------------------


def rows(path):
    """The tab-separated rows of one of this directory's .tsv files."""
    for line in path.read_text().splitlines():
        if line and not line.startswith("#"):
            yield line.split("\t")


def read_pin():
    pin = dict(rows(HERE / "pin.tsv"))
    if not re.fullmatch(r"[0-9a-f]{40}", pin.get("commit", "")):
        raise Refused("pin.tsv names no full commit hash")
    return pin["commit"], pin["url"]


def read_ranges(path=HERE / "extract.tsv"):
    ranges = []
    for name, src, first, last, first_text, last_text in rows(path):
        ranges.append((name, src, int(first), int(last), first_text, last_text))
    return ranges


def readme_departures():
    """The bold lead of each bullet under the parse README's Departures."""
    text = PARSE_README.read_text()
    section = re.search(r"^## Departures\n(.*?)^## ", text, re.M | re.S)
    if not section:
        raise Refused(f"{PARSE_README} has no Departures section")
    return re.findall(r"^- \*\*(.+?)\*\*", section.group(1), re.M)


def read_departures():
    """The expected departures, checked against the parse README's list."""
    expected = {lead: dep for dep, lead, _ in rows(HERE / "departures.tsv")}
    listed = readme_departures()
    missing = [lead for lead in listed if lead not in expected]
    stale = [lead for lead in expected if lead not in listed]
    if missing or stale:
        raise Refused(
            "departures.tsv and the parse README disagree:"
            + "".join(f"\n  the README lists, the oracle does not expect: {l}" for l in missing)
            + "".join(f"\n  the oracle expects, the README does not list: {l}" for l in stale)
        )
    return [expected[lead] for lead in listed]


# --- the C reference at the pin --------------------------------------------


def git(*args, cwd=None, binary=False):
    out = subprocess.run(["git", *args], cwd=cwd, capture_output=True, check=False)
    if out.returncode:
        raise Refused(f"git {' '.join(args)}: {out.stderr.decode().strip()}")
    return out.stdout if binary else out.stdout.decode()


def reference(libtsm, out, commit, url):
    """A git directory holding the pinned commit: the libtsm checkout if it
    has it, else a clone of the pin's URL under out/. Never writes to the
    checkout."""
    def has(gitdir):
        found = subprocess.run(
            ["git", "-C", str(gitdir), "rev-parse", "--verify", "--quiet", commit + "^{commit}"],
            capture_output=True, text=True, check=False)
        return found.stdout.strip() == commit

    if libtsm.is_dir() and has(libtsm):
        return libtsm
    clone = out / "libtsm.git"
    if not clone.is_dir():
        git("clone", "--quiet", "--bare", "--filter=blob:none", url, str(clone))
    if not has(clone):
        git("-C", str(clone), "fetch", "--quiet", "--filter=blob:none", url, commit)
    if not has(clone):
        raise Refused(f"{url} has no commit {commit}")
    return clone


def braces(lines):
    """The brace depth at the end of C lines, literals and comments aside."""
    text = re.sub(r"/\*.*?\*/|//[^\n]*|\"(?:\\.|[^\"\\])*\"|'(?:\\.|[^'\\])*'",
                  "", "\n".join(lines), flags=re.S)
    return text.count("{") - text.count("}")


def extract(gitdir, commit, ranges, dest):
    """Writes each range to dest/<name>.inc, refusing one whose edges moved."""
    dest.mkdir(parents=True, exist_ok=True)
    files = {}
    for name, src, first, last, first_text, last_text in ranges:
        if src not in files:
            files[src] = git("-C", str(gitdir), "show", f"{commit}:{src}").split("\n")
        lines = files[src]
        if not 1 <= first <= last <= len(lines):
            raise Refused(f"{name}: {src} has no lines {first}-{last}")
        piece = lines[first - 1:last]
        for line_no, got, want in ((first, piece[0], first_text), (last, piece[-1], last_text)):
            if " ".join(got.split()) != want:
                raise Refused(f"{name}: {src}:{line_no} reads {got.strip()!r}, "
                              f"extract.tsv expects {want!r}")
        if braces(piece):
            raise Refused(f"{name}: {src}:{first}-{last} does not balance its braces")
        (dest / f"{name}.inc").write_text(f'#line {first} "{src}"\n' + "\n".join(piece) + "\n")


def corpus(gitdir, commit):
    """The C reference's conformance inputs at the pin: (name, bytes)."""
    listing = git("-C", str(gitdir), "ls-tree", "-r", "--name-only", commit,
                  "test/conformance/cases", "test/conformance/recorded")
    names = sorted(n for n in listing.split() if n.endswith(".bin"))
    return [(n, git("-C", str(gitdir), "show", f"{commit}:{n}", binary=True)) for n in names]


# --- building both sides ---------------------------------------------------


def run_tool(cmd, **kw):
    out = subprocess.run(cmd, capture_output=True, text=True, check=False, **kw)
    if out.returncode:
        raise Refused(f"{' '.join(map(str, cmd))}:\n{out.stderr.strip()}")


def build_c(out):
    binary = out / "oracle-c"
    run_tool(["gcc", "-std=gnu11", "-O2", "-Wall", "-Werror",
              "-I", str(out / "extract"), "-o", str(binary), str(HERE / "harness" / "oracle.c")])
    return binary


def build_rust(manifest, target):
    run_tool(["cargo", "build", "--quiet", "--release", "--manifest-path", str(manifest)],
             env={**os.environ, "CARGO_TARGET_DIR": str(target)})
    return target / "release" / "parse-oracle"


# --- planted defects in vtium-parse ----------------------------------------

# Each plant is one exact replacement in a copy of vtium-parse's source; the
# run must turn red on every one. Those marked by an event only show with
# that event in the scripts.
PLANTS = {
    "gr-mapping-drops-csi-ignore": (
        "src/parser.rs",
        "                | State::CsiInt\n                | State::CsiIgnore\n",
        "                | State::CsiInt\n"),
    "csi-int-takes-digits": (
        "src/parser.rs",
        "            0x30..=0x3f => (Some(S::CsiIgnore), A::None),\n"
        "            0x40..=0x7e => (Some(S::Ground), A::CsiDispatch),\n"
        "            _ => (Some(S::CsiIgnore), A::None),\n        },\n        S::CsiIgnore",
        "            0x30..=0x3f => (None, A::None),\n"
        "            0x40..=0x7e => (Some(S::Ground), A::CsiDispatch),\n"
        "            _ => (Some(S::CsiIgnore), A::None),\n        },\n        S::CsiIgnore"),
    "dcs-run-takes-del": (
        "src/parser.rs",
        "    b < 0x7f && b != 0x18",
        "    b <= 0x7f && b != 0x18"),
    "osc-max-off-by-one": (
        "src/bounds.rs",
        "pub const OSC_MAX: usize = OSC.value;",
        "pub const OSC_MAX: usize = OSC.value - 1;"),
    "xtgettcap-max-off-by-one": (
        "src/bounds.rs",
        "pub const XTGETTCAP_MAX: usize = XTGETTCAP.value;",
        "pub const XTGETTCAP_MAX: usize = XTGETTCAP.value - 1;"),
    "subparams-off-by-one": (
        "src/params.rs",
        "if usize::from(*sub_len) < SUBPARAMS_MAX {",
        "if usize::from(*sub_len) + 1 < SUBPARAMS_MAX {"),
    "value-cap-off-by-one": (
        "src/params.rs",
        "v if v > PARAM_VALUE_CAP => v,",
        "v if v >= PARAM_VALUE_CAP => v,"),
    "utf8-accepts-c0-c1": (
        "src/utf8.rs",
        "            0xc2..=0xdf => (1, 0x80, 0xbf),",
        "            0xc0..=0xdf => (1, 0x80, 0xbf),"),
    # shows only with pause events
    "pause-ignored": (
        "src/parser.rs",
        "            if consumer.paused() {",
        "            if consumer.paused() && rest.is_empty() {"),
    # shows only with reset events
    "reset-keeps-pending-end": (
        "src/parser.rs",
        "        self.pending_end = None;\n        self.state = State::Ground;",
        "        self.state = State::Ground;"),
    # shows only with a mode switch inside a call
    "mode-read-once-per-call": (
        "src/parser.rs",
        "        let mut rest = bytes;\n"
        "        while let Some((&b, after_one)) = rest.split_first() {\n"
        "            let mode = consumer.input_mode();\n",
        "        let mut rest = bytes;\n"
        "        let mode = consumer.input_mode();\n"
        "        while let Some((&b, after_one)) = rest.split_first() {\n"),
}


def build_planted(name, out):
    """Builds the Rust side against a copy of vtium-parse with one plant."""
    src, old, new = PLANTS[name]
    root = out / "plants" / name
    shutil.rmtree(root, ignore_errors=True)
    parse = root / "crates" / "vtium-parse"
    shutil.copytree(REPO / "crates" / "vtium-parse" / "src", parse / "src")
    shutil.copy(REPO / "crates" / "vtium-parse" / "Cargo.toml", parse)
    shutil.copy(REPO / "Cargo.toml", root)
    tool = root / "tools" / "parse-oracle"
    shutil.copytree(HERE / "src", tool / "src")
    shutil.copy(HERE / "Cargo.toml", tool)
    shutil.copy(HERE / "Cargo.lock", tool)
    path = parse / src
    text = path.read_text()
    if text.count(old) != 1:
        raise Refused(f"plant {name}: its text is not in {src} exactly once")
    path.write_text(text.replace(old, new))
    return build_rust(tool / "Cargo.toml", out / "plants" / "target")


# --- event scripts ---------------------------------------------------------

MODES = b"u87"


def stream(mode, *records):
    """One stream's script: S, then its records (see the README)."""
    return b"S" + bytes([mode]) + b"".join(records)


def data(b):
    return b"D" + struct.pack("<I", len(b)) + b


RESET = b"R"


def mode_switch(m):
    return b"M" + bytes([m])


def pause_after(k, reset=False):
    return (b"Q" if reset else b"P") + struct.pack("<I", k)


def mode_after(k, m):
    return b"T" + struct.pack("<I", k) + bytes([m])


# Pieces of input the token generator joins: controls, introducers, UTF-8
# well- and ill-formed, and runs at each bound of the parse README's table.
TOKENS = [
    b"\x1b", b"[", b"]", b"P", b"_", b"X", b"^", b"\\", b"\x07", b";", b":", b"?", b">", b"<",
    b"=", b"!", b"$", b" ", b'"', b"+", b"#", b"(", b"%", b"q", b"m", b"p", b"h", b"c", b"G",
    b"0", b"1", b"4", b"9", b"38:2::255:0:0", b"\x18", b"\x1a", b"\x7f", b"\r", b"\n", b"\x00",
    b"\x9b", b"\x9c", b"\x9d", b"\x90", b"\x9f", b"\x98", b"\x85", b"\xc2\x9b", b"\xc2\x9c",
    b"\xc2\x9d", b"\xc2\x90", b"\xc2\x9f", b"\xc2\x98", b"\xc3\xa9", b"\xe2\x82\xac", b"\xe2\x82",
    b"\xf0\x9f\x98\x80", b"\xf0\x9f", b"\xc0\x9b", b"\xc1\x81", b"\xe0\x80\x9b", b"\xed\xa0\x80",
    b"\xef\xbf\xbf", b"\xef\xb7\x90", b"\xf4\x90\x80\x80", b"\xf7\xbf\xbf\xbf", b"\xff", b"\x80",
    b"\xa0", b"\xbb", b"\xdc", b"\xe9", b"\x1bP+q", b"\x1bP++q", b"544e;436f", b"\x1b]", b"\x1b_G",
    b"\x1bPq", b"\x1b[?", b"\x1b\\", b"text", b"a=T,f=32;", b"AAAA",
    # PARAMS_MAX, SUBPARAMS_MAX and PARAM_VALUE_CAP
    b"1;" * 15, b"1;" * 16, b";" * 17, b"1:" * 8, b"1:" * 9, b":" * 9,
    b"65535", b"65536", b"655359", b"6553599", b"99999999",
    # INTERMEDIATES_MAX
    b"\x1b[!!p", b"\x1b[ !\"p", b"\x1b(%%0", b"\x1b[1 !\"m", b"\x1bP !\"q",
    # OSC_MAX: a byte short, at, and past it, and a character across it
    b"\x1b]" + b"x" * 126 + b"\x07", b"\x1b]" + b"x" * 127 + b"\x07",
    b"\x1b]" + b"x" * 128 + b"\x07", b"\x1b]" + b"x" * 126 + b"\xc3\xa9y\x07",
    b"\x1b]" + b"x" * 125 + b"\xe2\x82\xacy\x1b\\",
    # XTGETTCAP_MAX
    b"\x1bP+q" + b"5" * 1038 + b"\x1b\\", b"\x1bP+q" + b"5" * 1039 + b"\x1b\\",
    b"\x1bP+q" + b"5" * 1040 + b"\x9c",
]

TERMINATORS = [b"\x1b\\", b"\x07", b"\x9c", b"\xc2\x9c", b"\x18", b"\x1a", b"\x1b[", b"\x1bX",
               b"\x1b\x1b\\", b"\x85", b"\xc2\x85", b""]
INTRODUCERS = {
    "csi": [b"\x1b[", b"\x9b", b"\xc2\x9b"],
    "dcs": [b"\x1bP", b"\x90", b"\xc2\x90"],
    "osc": [b"\x1b]", b"\x9d", b"\xc2\x9d"],
    "apc": [b"\x1b_", b"\x9f", b"\xc2\x9f"],
    "sos": [b"\x1bX", b"\x1b^", b"\x98", b"\x9e"],
}
TEXT = [b"a", b"Z", b" ", b"0", b";", b"\x7f", b"\t", b"\n", b"\x05", b"\xc3\xa9", b"\xe2\x82\xac",
        b"\xf0\x9f\x98\x80", b"\xef\xbf\xbe", b"\xe2\x82", b"\xa9", b"\xff", b"\xc0\x80"]


def params(rng):
    def value():
        return rng.choice([b"", b"", b"0", b"1", b"5", b"38", b"65535", b"65536", b"655359",
                           b"00012", b"9999999"])
    out = []
    for _ in range(rng.choice([0, 1, 1, 2, 3, 5, 15, 16, 17, 20])):
        p = value()
        for _ in range(rng.choice([0, 0, 0, 1, 2, 8, 9])):
            p += b":" + value()
        out.append(p)
    return b";".join(out)


def header(rng, kind):
    """A CSI or DCS header after its introducer: marker, parameters,
    intermediates, final, sometimes malformed."""
    h = b""
    if rng.random() < 0.3:
        h += bytes([rng.choice(b"<=>?")])
    h += params(rng)
    if rng.random() < 0.1:
        h += bytes([rng.choice(b"<=>?:")])
    h += bytes(rng.choice(b" !\"#$%&'()*+,-./") for _ in range(rng.choice([0, 0, 0, 1, 1, 2, 3])))
    if rng.random() < 0.05:
        h += rng.choice(TEXT)
    finals = b"qpq|" if kind == "dcs" else b"mHJKhlrptcqn@~"
    h += bytes([rng.choice(finals) if rng.random() < 0.8 else rng.randrange(0x40, 0x7f)])
    return h


def payload(rng, n):
    return b"".join(rng.choice(TEXT) for _ in range(n))


def sequence(rng):
    kind = rng.choice(["csi", "csi", "esc", "dcs", "osc", "osc", "apc", "sos", "text", "c0"])
    if kind == "text":
        return payload(rng, rng.randrange(1, 20))
    if kind == "c0":
        return bytes([rng.choice(b"\x00\x05\x07\x08\x0a\x0d\x0e\x0f\x18\x1a\x84\x85\x9c")])
    if kind == "esc":
        ims = bytes(rng.choice(b" !\"#$%&'()*+,-./") for _ in range(rng.choice([0, 0, 1, 2, 3])))
        return b"\x1b" + ims + bytes([rng.randrange(0x30, 0x7f)])
    intro = rng.choice(INTRODUCERS[kind])
    if kind == "csi":
        return intro + header(rng, kind)
    end = rng.choice(TERMINATORS)
    if kind == "dcs":
        if rng.random() < 0.3:
            return intro + b"+q" + b";".join(b"544e" * rng.randrange(1, 4)
                                             for _ in range(rng.randrange(1, 4))) + end
        return intro + header(rng, kind) + payload(rng, rng.randrange(0, 60)) + end
    if kind == "osc":
        n = rng.choice([0, 5, 40, 120, 125, 126, 127, 128, 140])
        return intro + b"0;" + b"x" * n + payload(rng, rng.randrange(0, 4)) + end
    if kind == "apc":
        return intro + b"Ga=T,f=32;" + payload(rng, rng.randrange(0, 40)) + end
    return intro + payload(rng, rng.randrange(0, 20)) + end


def body(rng):
    k = rng.randrange(5)
    if k == 0:
        return bytes(rng.randrange(256) for _ in range(rng.randrange(1, 3000)))
    if k in (1, 2):
        return b"".join(rng.choice(TOKENS) for _ in range(rng.randrange(1, 1500)))
    return b"".join(sequence(rng) for _ in range(rng.randrange(1, 200)))


def with_events(rng, mode, b):
    """b cut into records, with resets, mode switches, pauses and mode
    triggers between them; half the cuts fall right after an ESC, where a
    string's end waits."""
    escs = [i + 1 for i, c in enumerate(b) if c == 0x1b]
    cuts = set()
    for _ in range(rng.randrange(0, 12)):
        if escs and rng.random() < 0.5:
            cuts.add(rng.choice(escs))
        else:
            cuts.add(rng.randrange(len(b) + 1))
    records, start = [], 0
    for cut in sorted(cuts) + [len(b)]:
        for _ in range(rng.choice([0, 0, 1, 1, 2])):
            r = rng.random()
            if r < 0.3:
                records.append(RESET)
            elif r < 0.45:
                records.append(mode_switch(rng.choice(MODES)))
            elif r < 0.7:
                records.append(pause_after(rng.randrange(1, 30), reset=rng.random() < 0.3))
            else:
                records.append(mode_after(rng.randrange(1, 10), rng.choice(MODES)))
        records.append(data(b[start:cut]))
        start = cut
    return stream(mode, *records)


def generated(seed, n):
    rng = random.Random(seed)
    for i in range(n):
        mode = rng.choice(b"uu87")
        b = body(rng)
        if rng.random() < 0.3:
            yield f"generated seed {seed} #{i}", stream(mode, data(b))
        else:
            yield f"generated seed {seed} #{i}", with_events(rng, mode, b)


# Scripts written for the edges a random cut reaches rarely.
CASES = {
    "apc-reset-at-st": stream(ord("u"), data(b"\x1b_Ga=T;AAAA\x1b"), RESET, data(b"\\x")),
    "osc-reset-at-st": stream(ord("u"), data(b"\x1b]0;title\x1b"), RESET, data(b"\\x")),
    "dcs-reset-at-st": stream(ord("u"), data(b"\x1bPqdata\x1b"), RESET, data(b"\\x")),
    "xtgettcap-reset-at-st": stream(ord("u"), data(b"\x1bP+q544e\x1b"), RESET, data(b"\\x")),
    "apc-reset-twice": stream(ord("u"), data(b"\x1b_Gx\x1b"), RESET, RESET, data(b"\\x")),
    "decscl-inside-a-call": stream(
        ord("u"), mode_after(1, ord("8")), data(b"\x1b[61\"p\xe9\x9b1m\xc3\xa9" * 3)),
    "mode-mid-character": stream(
        ord("u"), data(b"\xc3"), mode_switch(ord("8")), data(b"\xa9"),
        mode_switch(ord("u")), data(b"\xa9\xe2\x82"), mode_switch(ord("7")), data(b"\xac\xe2"),
        mode_switch(ord("u")), data(b"\x82\xac")),
    "pause-and-resume": stream(
        ord("u"), pause_after(2), data(b"\x1b[1mab\x1b_Gx\x1b\\cd"), pause_after(1, reset=True),
        data(b"\x1b]0;t\x07\x1b[2m")),
}


def scripts(src, seed, n):
    """Every stream a run compares: (label, script)."""
    for name, b in src.corpus:
        for mode in MODES:
            yield f"{name} mode {chr(mode)}", stream(mode, data(b))
        rng = random.Random(zlib.crc32(name.encode()))
        yield f"{name} with events", with_events(rng, ord("u"), b)
    for name, script in CASES.items():
        yield f"case {name}", script
    yield from generated(seed, n)


# --- the differential ------------------------------------------------------


def trace(cmd, script):
    out = subprocess.run(cmd, input=script, capture_output=True, check=False)
    if out.returncode:
        raise Refused(f"{cmd[0]}: {out.stderr.decode().strip()}")
    return out.stdout.split(b"\n")


def compare(c_side, rust, batches, save):
    """Runs both sides over the batches; returns (streams, lines, difference)
    with the first difference found, or None."""
    streams = lines = 0
    for batch in batches:
        script = b"".join(s for _, s in batch)
        a, b = trace(c_side, script), trace([str(rust)], script)
        if a != b:
            i = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
            n = sum(1 for line in a[:i + 1] if line.startswith(b"stream ")) - 1
            label, one = batch[n]
            save.write_bytes(one)
            start = max(i - 3, max(j for j in range(i + 1) if a[j].startswith(b"stream ")))
            return streams + n, lines, (label, i, a[start:i + 3], b[start:i + 3])
        streams += len(batch)
        lines += len(a) - 1
    return streams, lines, None


def batched(items, size=200):
    batch = []
    for item in items:
        batch.append(item)
        if len(batch) == size:
            yield batch
            batch = []
    if batch:
        yield batch


class Source:
    """The pinned C reference, extracted and built under out/."""

    def __init__(self, args):
        self.out = args.out.resolve()
        self.out.mkdir(parents=True, exist_ok=True)
        self.commit, url = read_pin()
        self.gitdir = reference(args.libtsm.resolve(), self.out, self.commit, url)
        extract(self.gitdir, self.commit, read_ranges(), self.out / "extract")
        self.c = build_c(self.out)
        self.corpus = corpus(self.gitdir, self.commit)


def differential(src, rust, departures, seed, n):
    started = time.monotonic()
    streams, lines, diff = compare(
        [str(src.c), *departures], rust, batched(scripts(src, seed, n)),
        src.out / "difference.script")
    took = time.monotonic() - started
    return streams, lines, diff, took


def report_difference(diff, out):
    label, line, c_lines, rust_lines = diff
    print(f"DIFFERENCE in {label}, at trace line {line + 1}")
    print("  C reference:", *(l.decode(errors="replace") for l in c_lines), sep="\n    ")
    print("  vtium-parse:", *(l.decode(errors="replace") for l in rust_lines), sep="\n    ")
    print(f"  the stream's script: {out / 'difference.script'}")


def cmd_run(args):
    expected = read_departures()
    departures = [d for d in expected if d not in args.without]
    src = Source(args)
    if args.plant:
        rust = build_planted(args.plant, src.out)
    else:
        rust = build_rust(HERE / "Cargo.toml", HERE / "target")
    print(f"C reference: libtsm {src.commit[:12]}, {len(read_ranges())} ranges extracted")
    print(f"departures expected: {', '.join(departures) or 'none'}")
    if args.plant:
        print(f"planted in vtium-parse: {args.plant}")
    streams, lines, diff, took = differential(src, rust, departures, args.seed, args.streams)
    if diff:
        report_difference(diff, src.out)
        return 1
    print(f"agree: {streams} streams, {lines} trace lines, no unlisted difference ({took:.1f}s)")
    return 0


def cmd_selftest(args):
    expected = read_departures()
    src = Source(args)
    rust = build_rust(HERE / "Cargo.toml", HERE / "target")
    failures = []

    def expect(what, red, detail):
        verdict = "red" if red else "GREEN"
        print(f"  {verdict:5}  {what}: {detail}")
        if not red:
            failures.append(what)

    print("baseline, every departure expected:")
    streams, lines, diff, _ = differential(src, rust, expected, args.seed, args.streams)
    if diff:
        report_difference(diff, src.out)
        print("selftest: the baseline differs; nothing else is meaningful")
        return 1
    print(f"  agree  {streams} streams, {lines} trace lines")

    print("each departure taken out of the expectations:")
    for dep in expected:
        streams, _, diff, _ = differential(
            src, rust, [d for d in expected if d != dep], args.seed, args.streams)
        expect(dep, diff, f"differs at stream {streams + 1}, {diff[0]}" if diff else "no difference")

    print("each defect planted in vtium-parse:")
    for name in PLANTS:
        planted = build_planted(name, src.out)
        streams, _, diff, _ = differential(src, planted, expected, args.seed, args.streams)
        expect(name, diff, f"differs at stream {streams + 1}, {diff[0]}" if diff else "no difference")

    print("each range moved a line up and a line down:")
    ranges = read_ranges()
    for i, (name, src_path, first, last, first_text, last_text) in enumerate(ranges):
        extracted = []
        for shift in (-1, 1):
            moved = list(ranges)
            moved[i] = (name, src_path, first + shift, last + shift, first_text, last_text)
            try:
                extract(src.gitdir, src.commit, moved, src.out / "moved")
                extracted.append(f"{shift:+d}")
            except Refused:
                pass
        expect(name, not extracted,
               f"extracted at {', '.join(extracted)}" if extracted else "refused both")

    if failures:
        print(f"selftest: {len(failures)} not caught: {', '.join(failures)}")
        return 1
    print("selftest: every plant, departure and moved range caught")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    for name, func in (("run", cmd_run), ("selftest", cmd_selftest)):
        p = sub.add_parser(name)
        p.set_defaults(func=func)
        p.add_argument("--libtsm", type=Path, default=REPO.parent / "libtsm",
                       help="a libtsm checkout holding the pinned commit, read only "
                            "(default: ../libtsm beside this repository; without the "
                            "commit, the pin's URL is cloned under --out)")
        p.add_argument("--out", type=Path, default=HERE / "out",
                       help="where the extraction, builds and differences go")
        p.add_argument("--seed", type=int, default=1, help="the generator's seed")
        p.add_argument("--streams", type=int, default=3000,
                       help="how many generated streams follow the corpus")
    run = sub.choices["run"]
    run.add_argument("--without", action="append", default=[], metavar="DEPARTURE",
                     help="take a departure out of the expectations")
    run.add_argument("--plant", choices=sorted(PLANTS),
                     help="build vtium-parse with a planted defect")
    args = parser.parse_args()
    try:
        return args.func(args)
    except Refused as e:
        print(f"oracle: refused: {e}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
