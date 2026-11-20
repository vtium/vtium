"""The batch file each tier 3 variant installs, /etc/vtium/batch.conf.

vtium draws only the fonts that the batch lists. When /etc/vtium/batch.conf
is missing, the batch is the list of a fresh install: the font files the
package ships in /usr/share/vtium/fonts, the console font that vconsole.conf
names, the color schemes in /usr/share/vtium/palettes, and the pointer file
/usr/share/vtium/pointers.svg. A font beyond that list counts as missing
until root lists it, so a variant that draws one needs the batch file that
an administrator would write for it.

This module works out from the variant itself which font files it can draw:
each `font-name` line of the configuration the guest installs (a font file,
a frit or a family), each frit the disc carries, every source of those frits
and of the layers they name. A frit is looked up among the guest's frits and
then among the frits vtium ships. A `font:` source names a family, which the
helper finds in the guest by its name, so FAMILIES gives the files of each
family in the guest image. A source this module cannot place, such as a
family FAMILIES lacks, stops the run, so a new variant cannot leave a font
out of its batch.

A variant whose fonts are all in the fresh-install list writes no batch
file, so tier 3 keeps checking how vtium starts without one. Any other
variant writes the fresh-install list, read from the run's install tree,
followed by the fonts it draws beyond it. The guest's vconsole.conf names no
FONT=, so the list holds no console font.

References:
- ADR-1205 D4: the batch, its file, and the list of a fresh install.
"""

import os
import re
import tarfile

from t3_common import GUEST, ROOT

# The batch file vtium reads, and the folders of the package's data in the
# install tree.
BATCH_FILE = "/etc/vtium/batch.conf"
PACKAGE_FONTS = "/usr/share/vtium/fonts"
PACKAGE_PALETTES = "usr/share/vtium/palettes"
POINTERS = "usr/share/vtium/pointers.svg"
# The source that holds the fresh-install list's font names, FRESH_FONTS.
BATCH_RS = os.path.join(ROOT, "crates", "vtium-settings", "src", "batch.rs")
# The frits and layers vtium ships, which a configuration names by name.
SHIPPED_FRITS = os.path.join(ROOT, "crates", "vtium-config", "frits")
# The set vtium draws when the configuration names no font.
DEFAULT_SET = "terminus"
# The font files of each family that a shipped layer names with `font:`, as
# the guest image holds them: noto-fonts-emoji installs Noto Color Emoji.
# The guest image holds no Symbols Nerd Font Mono, so the nerd-symbols layer
# draws nothing there.
FAMILIES = {
    "Noto Color Emoji": ["/usr/share/fonts/noto/NotoColorEmoji.ttf"],
    "Symbols Nerd Font Mono": [],
}


def fresh_fonts():
    """The names of the font files a fresh install activates in
    PACKAGE_FONTS, read from vtium-settings's FRESH_FONTS."""
    with open(BATCH_RS) as f:
        text = f.read()
    m = re.search(r"pub const FRESH_FONTS: \[&str; (\d+)\] = \[(.*?)\];", text, re.S)
    if not m:
        raise SystemExit("%s holds no FRESH_FONTS list" % BATCH_RS)
    names = re.findall(r'"([^"]+)"', m.group(2))
    if len(names) != int(m.group(1)):
        raise SystemExit("cannot read FRESH_FONTS in %s: %d names for %s"
                         % (BATCH_RS, len(names), m.group(1)))
    return names


def fresh_list(tar_path):
    """The list of a fresh install: the package's fonts,
    then each color scheme the install tree at `tar_path` puts in
    /usr/share/vtium/palettes, then its pointer file."""
    out = [PACKAGE_FONTS + "/" + name for name in fresh_fonts()]
    palettes = []
    pointers = []
    with tarfile.open(tar_path) as tar:
        for member in tar.getmembers():
            name = member.name.lstrip("./")
            if not member.isfile():
                continue
            if os.path.dirname(name) == PACKAGE_PALETTES:
                palettes.append("/" + name)
            elif name == POINTERS:
                pointers.append("/" + name)
    return out + sorted(palettes) + pointers


def frit_file(name):
    """The file of the frit `name`: the guest's (guest/frits), else the one
    vtium ships."""
    path = os.path.join(GUEST, "frits", name + ".vtfrit")
    if os.path.isfile(path):
        return path
    for folder, _, files in os.walk(SHIPPED_FRITS):
        if name + ".vtfrit" in files:
            return os.path.join(folder, name + ".vtfrit")
    raise SystemExit("no frit %s among tier 3's frits or the frits vtium ships, so tier 3 "
                     "cannot tell which fonts it draws" % name)


def layer_file(name):
    """The file of the layer `name` that vtium ships."""
    path = os.path.join(SHIPPED_FRITS, "layers", name + ".vtlayer")
    if not os.path.isfile(path):
        raise SystemExit("no layer %s among the layers vtium ships, so tier 3 cannot tell "
                         "which fonts it draws" % name)
    return path


def source_fonts(value, where):
    """The font files a frit's or a layer's `source=` value names."""
    kind, _, rest = value.partition(":")
    if kind == "builtin":
        return []
    if kind == "font":
        if rest not in FAMILIES:
            raise SystemExit("%s names the family %s, whose files in the guest "
                             "t3_batch.FAMILIES does not list" % (where, rest))
        return list(FAMILIES[rest])
    if kind in ("file", "bdf", "hex", "outline") and rest.startswith("/"):
        return [rest]
    raise SystemExit("%s holds the source %s, which tier 3 cannot place in the guest"
                     % (where, value))


def file_fonts(path, seen):
    """The font files the frit or layer at `path` draws, its layers'
    included. `seen` holds the files already read."""
    if path in seen:
        return []
    seen.add(path)
    out = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line.startswith("source="):
                out += source_fonts(line[len("source="):], os.path.relpath(path, ROOT))
            elif line.startswith("layer="):
                out += file_fonts(layer_file(line[len("layer="):]), seen)
    return out


def named_fonts(name, engine, seen):
    """The font files that the value `name` of a `font-name` line draws
    under the font engine `engine`."""
    if engine in ("unifont", "8x16"):
        return []
    if name.startswith("/"):
        return [name]
    if engine in ("freetype", "pango"):
        return source_fonts("font:" + name, "font-name=" + name)
    return file_fonts(frit_file(name), seen)


def variant_fonts(variant, conf_text):
    """The font files `variant` can draw: those its configuration
    `conf_text` names, with the frits of its looks, and those of each frit
    its disc carries."""
    names = []
    engine = None
    for line in conf_text.splitlines():
        line = re.sub(r"\s+#.*$", "", line.strip())
        key, eq, value = line.partition("=")
        if not eq or line.startswith("#"):
            continue
        key, value = key.strip(), value.strip()
        if key == "font-name" and value:
            names.append(value)
        elif key == "font-engine":
            engine = value
    seen = set()
    out = []
    for name in names or [DEFAULT_SET]:
        out += named_fonts(name, engine, seen)
    for frit in variant.get("frits", []) + variant.get("deferred_frits", []):
        out += file_fonts(frit_file(frit[:-len(".vtfrit")]), seen)
    return list(dict.fromkeys(out))


def batch(variant, conf_text, tar_path):
    """The batch file `variant` installs: None when the list of a fresh
    install holds every font it draws, else that list, read from the
    install tree at `tar_path`, followed by the fonts it draws beyond it."""
    fresh = fresh_list(tar_path)
    extra = [f for f in variant_fonts(variant, conf_text) if f not in fresh]
    if not extra:
        return None
    lines = ["# Written by tier 3 (tools/tier3/t3_batch.py): the list of a fresh install,",
             "# then the fonts this variant draws beyond it."]
    return "\n".join(lines + fresh + extra) + "\n"
