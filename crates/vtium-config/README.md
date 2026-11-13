# vtium-config

vtium-config is vtium's configuration tool, which ADR-1201 and ADR-1403 decide. Users reach it as `vtium config`. vtium-config is also the program that generates vtium's glyphs at run time (ADR-1200, ADR-1301 D2). It is a binary of its own, outside the minspec build.

`vtium-config tui` opens its TUI, a full-screen program drawn in the terminal's text cells. The TUI does these things:

- It draws the configuration as a flow, from the displays to the input devices.
- It lists the frits (font set files) and the built-in fonts.
- It switches the live console to each font that the user highlights.
- It previews sizes as images.
- It writes every change into the user's draft of vtium.conf, which Install installs.

The `serve` command is the glyph helper that the console starts. The helper takes a frit's text from the console, and it finds and reads the frit's sources. It parses each bitmap font and rasterises each outline font. It sends the glyphs at the cell asked for, in the glyph chunk format, written with vtium-font's writer.

The `use-frit` command switches the running console's frit. The flow commands, `flow`, `rule`, `head`, `look`, `set`, `install` and the rest, edit the user's draft of vtium.conf and install it. See [The draft](#the-draft) for details.

Every decoder lives here. The console reads only the helper's glyphs and its own built-in fonts. The crate forbids unsafe code. The console font ioctls that it needs live in vtium-platform's `src/vt/font.rs`.

| Path | What it is |
|---|---|
| `src/main.rs` | The binary: the command line, or the parser when run as `vtium-config --parse`. |
| `src/cli.rs` | The command line: its options, the commands it dispatches, and their exit codes. |
| `src/commands.rs` | `fonts`, `check FILE`, `remove-frit NAME` and `new-frit`'s non-interactive forms (ADR-1201 D2), and the key checks that `get` and `set` share. |
| `src/store.rs` | vtium-config's store under the XDG directories (ADR-1403 D8). It holds the drafts, the library, the tool's settings, the installed history and each draft's base. It finds root's home from the password database. |
| `src/drafts.rs` | The current draft, made from the installed file when there is none. The draft's state against the installed file, the check that every change runs, and `install`. |
| `src/vts.rs` | The step after an install of the system's vtium.conf (ADR-1201 D5). It reads `vtium console status`. It enables vtium with `vtium console enable` on each VT that a head names, where vtium is not enabled yet. It runs the `systemctl start` and `restart` that apply the new file, only after a yes. |
| `src/ops.rs` | The one set of operations on the draft. Each operation takes vtium.conf's text and gives back the edited text, through vtium-settings's line editor and section writer. The commands and the TUI call them alike. |
| `src/flowmodel.rs` | The Flow view's model, built from a draft and the devices present, and placed through vtium-devices. Also `flow`'s text and JSON. |
| `src/flowcli.rs` | The flow's commands and their exit codes. |
| `src/outcome.rs` | The exit codes 1 to 5 of ADR-1403 D8. |
| `src/library.rs` | The library, `library.conf`. Looks and heads are copied between the library and the draft. |
| `src/tui/` | The TUI (ADR-1201 D3, D4, D7; ADR-1403 D7). `model.rs` is the state machine: it takes keys, mouse clicks, replies, resizes, signals and the end of a wait, and returns what the loop does. `view.rs` draws the TUI's screen on `canvas.rs`'s cells, and gives the bytes that draw it. `flow.rs` holds the flow read from the draft and the devices present, as plain data. `paths.rs` holds that flow as paths, and the plan of a path's changes. `parts.rs` holds the parts that a path is made from. `flowview.rs` lays out the Flow view's document of CONFIGURATION, NEW PATH and PARTS. `flowdraw.rs` draws them: a terminal's card and its subflow. `flowkeys.rs` holds the Flow view's keys. `flowedit.rs` holds Save, Install, Revert, Restore and New with their questions, and the file view. `flowcolor.rs` holds the colors that the person sets in `vtium-config.conf`. `color.rs` holds the sixteen colors that the TUI draws in. `input.rs` is the decoder of keys, mouse reports and replies. `words.rs` is the catalog of what the TUI says. `console.rs` holds the preview that a vtium console draws as its own text, and the save of the font that it shows. `port.rs` is the one loop and the real terminal, raw on the alternate screen with its signals held. `fixture.rs` is the flow of ADR-1403 D7's figure that the tests draw. `mod.rs` is what the TUI starts from. |
| `src/draft.rs` | What an install bundle changes: each file before and after, each frit removed, and the lines that differ. The install view and `diff` show these. |
| `src/apply.rs` | The install bundle that an install becomes, and the argv of the privileged step. `apply DIR` checks an install bundle and installs it. `make` is what a frit command's change goes through: an install bundle made as a user, and installed at once as root. |
| `src/display.rs` | The connected displays as sysfs shows them: each display's first mode, and the size that its EDID states. The TUI and `preview` take the density and the grid from these. vtium-platform's reader reads the EDID, and the console's own `Density::of` works out the density. So the tool's density and the console's density are one. |
| `src/kitty.rs` | Kitty graphics' direct transmission of raw pixels: the bytes that show and delete a size's image. |
| `src/preview.rs` | `preview FONT...`: each font's specimen. The specimen is an image where the terminal answers kitty's graphics query, and a line of text otherwise (ADR-1201 D9). |
| `src/specimen.rs` | The glyph helper as vtium-config starts it, under the helper's own sandbox. It gives a frit's specimen at a size, laid out from the glyphs that the helper answers with. It also gives the sources that a frit lacks, which the helper finds. |
| `src/frit.rs` | Frits in vtium.conf's line syntax, read strictly. |
| `src/sources.rs` | Each source, found and read: kbd names as `setfont` finds them, files, vconsole.conf and the kernel command line, the kernel's VT font behind the `Consoles` seam, and the built-ins. |
| `src/child.rs` | The parser's child process, `--parse`: its limits and the request it reads. Its parent's side lives in `tests/common/parser.rs`. `preview`, `fonts` and the TUI reach fonts through the helper, `serve`, under its own sandbox. |
| `src/decode.rs` | The decoders that the helper and the parser run: compressions told apart by their magic, gzip, PSF through vtium-font's reader, the font of a CPI file that `cp-size=` takes, and kbd unimaps. |
| `src/bdf.rs` | vtium-config's own BDF reader. It gives a `bdf:` source's glyphs at the font's own cell. |
| `src/cover.rs`, `src/cover.rs.sha256` | The named `cover=` sets as range tables, and their SHA-256. `tools/cover-tables` generates them, and nobody edits them by hand. |
| `src/outline.rs` | Outline fonts: the request, and the bounded answer that the parent checks. Also the rasteriser that the parser runs, where swash places each glyph in its cell as R8 or premultiplied RGBA. |
| `src/shaping/` | Clusters (vtium/vtium#144). Each cluster is judged by what it holds, shaped with HarfRust, and drawn whole by swash. `data.rs` holds the Unicode tables that `gen-shaping/` generates, and nobody edits it by hand. |
| `src/made.rs` | How the glyph helper made each superscript and subscript glyph, which the font preview's caption names (ADR-1202 D14). Also the frame that carries that record. |
| `src/resample.rs` | Scaling a color bitmap down to its cells, by the method chosen when the tool is built. |
| `src/glyphs.rs` | The bounded glyph form that the parser answers with, and its check. |
| `src/font.rs` | A bitmap source's glyphs at its own cell, from a parsed font, a BDF font or a built-in one. |
| `src/serve.rs` | `serve`: the helper that the console starts to generate a frit's glyphs at run time. Its request, its framed answers and its idle exit. |
| `src/switch.rs` | `use-frit`: OSC 50 and its query on the controlling terminal. The reply is read in raw mode, within its wait. |
| `src/report/` | `report` (ADR-1200 D24). `parts.rs` gathers each part of the document. `binary.rs` reads each vtium binary's version and features. `units.rs` finds the vtium units and reads their logs. `edid.rs` reads a monitor's name, size and serials. `read.rs` is the only way that the report reads files and runs programs, under the root that `--root` names. `scrub.rs` replaces the hostname and the monitors' serials in what was gathered. |
| `tests/cli.rs` | The command line's usage errors, the check that the store's commands are gone, and the parser's child under its limits. |
| `tests/engine.rs` | The helper's engine on each source form, decoder and shipped frit, with fonts, a kbd tree and font directories generated in the test. |
| `tests/parser.rs`, `tests/common/parser.rs` | The parser's child as its parent runs it. A child fails its source when it dies, fails, answers badly, overruns its deadline or floods stderr. The child holds an empty environment and only its pipes. A child that leaves a process holding its stdout is reaped as it exits, and that process is ended with it. |
| `tests/serve.rs` | `serve` run as the console runs it, under its own sandbox. |
| `tests/switch.rs` | `use-frit` on a real pty in raw mode, answered by vtium-term's terminal. Also the frits that it checks a name against, and its refusals before the terminal is touched. |
| `tests/tui.rs` | The TUI's real loop, driven headlessly. Scripted keys go in. On the other side is vtium-term, or a terminal that answers only DA1. The state's effects are read in the bytes written and on the screen. |
| `tests/tui_pty.rs` | The real binary's TUI on a pty, with vtium-term answering. It covers the start bytes, live switching with one switch in flight, the font given back on q, and Ctrl-C, SIGTERM, SIGHUP, SIGINT and SIGQUIT. It also covers an end when the terminal closes, no OSC 50 to another terminal, `use-frit`'s check of the terminal, and a whole install through a stand-in for sudo. |
| `tests/screens/` | The TUI's screens at 80×25 and 100×30, which are vtium's own goldens. `provisional.tsv` lists those that the maintainer has not yet accepted, which judge nothing. [Reviewing the screens](#reviewing-the-screens) says how to see them as images and in the TUI. |
| `tests/vts.rs` | The step after an install, with stand-ins for sudo, `vtium console` and `systemctl` that refuse a user who is not root. It checks that a VT that a head names is enabled once, in the mode that vtium's other VTs use. It checks that a refusal is shown and the install kept, and that nothing is started or restarted without a yes. It reads `vtium console status`'s table as the console lays it out. It checks that the TUI's desk and the command line's install run the same commands with the same words. `tests/tui_pty.rs` runs the same step through the real binary at a terminal, from the TUI and from `install`. |
| `tests/flow.rs` | The flow's commands, run as the binary against a fixture configuration directory and XDG home. A stand-in `vtium` refuses in its check each line that holds `BAD`. The tests cover placement, an unfinished draft written, and an install refused when the installed file changed since. They also cover an install and its history, `edit` with a stand-in editor, `flow --json` whole, the drafts' names, the usage errors and `--dry-run`. |
| `tests/commands.rs` | `fonts`, `check`, `remove-frit` and `new-frit`, run as the binary in a temporary directory standing in for `/etc/vtium`. Each change is drafted and installed by `apply`. |
| `tests/apply.rs` | `apply` in a temporary directory standing in for `/etc/vtium`: one changed line, a target changed since the draft, a check that fails, a write that fails before its rename, links and bounds. |
| `tests/report.rs` | `report` against stand-in trees of `/proc`, `/etc`, `/run` and `/sys`, and shell-script stand-ins for `vtium`, `systemctl` and `journalctl`. The tests cover each part, a journal that the user cannot read, and a journalctl that refuses `-F` with `-b`, as journalctl 262 does. They cover a journal with no boot before this one, a log run that fails, a machine with no card, root against a user, and root's save refused by the command itself. They cover the previous boot, `--save`, the console's own `--version` lines, and vtium-config's binary carrying no feature marker. They also plant a hostname and serials, and check that these never appear. The serials are a connected monitor's, an unplugged one's, and those in the `edid:` and `serial:` tokens of vtium.conf's rules that `check-config` echoes. |
| `tests/specimen.rs` | Specimens drawn by the real helper, their clip to the preview area and their cap, and the painter on the built-in fonts and a shipped frit. |
| `tests/preview.rs` | `preview` run as the binary: a line of text to a pipe, its size from `--size` or vtium.conf, and a font that this machine lacks. Under `kitty-check`, it also checks an image for each font, on a pty whose console is vtium-term built with `images`. |
| `tests/kitty_bytes.rs` | The size images, as vtium-term built with `images` reads them. The tests run under the `kitty-check` feature, which the workspace's `--all-features` run turns on. They check that the graphics query is answered before DA1. They check that one placement follows an image sent in one command or in chunks, and that none remains after the delete. |
| `tests/outline.rs` | Outline rasterising on a TrueType font with COLR color glyphs, written by the TrueType test font writer in `tools/testfont/src/ttf.rs`. |
| `tests/shaping.rs` | Clusters shaped on a TrueType font with `ccmp`, joining forms and a precomposed letter written in the test. |
| `gen-shaping/` | The generator of `src/shaping/data.rs` from the Unicode Character Database. It is a workspace of its own, run by hand. |
| `fuzz/` | The fuzz targets, in a workspace of their own (`fuzz/README.md`). `shape` drives clusters through the shaper and the rasteriser. `bdf` drives any bytes through the BDF reader. `serve` drives any bytes through the glyph helper's request parser. |
| `frits/` | The frits vtium ships. `minspec-16` and `minspec-8` are minspec's curated symbols and emoji over the built-in fonts. `proof` is vtium/vtium#23's desktop frit. `tiny5` is an example of the smallest font: the pixel font Tiny5 Mono at its own 6×9 cell. It is never a default (vtium/vtium#217). |

`cargo test -p vtium-config` runs every test.

## Commands

| Command | Runs as | Does |
|---|---|---|
| none | any user | Prints the usage that `--help` prints, to stdout, and exits 0, at a terminal or not. |
| `tui` | the user | Opens the TUI ([The TUI](#the-tui)) at a terminal. Away from a terminal it is a usage error and exits 2. |
| `fonts` | any user | Lists the frits and the built-in fonts, one line each, tab-separated. Each line gives the name, then its state, then the sources that this machine lacks. The state is `reads`, `built in`, `does not read:` and why, or `draws nothing:` and why when no source of the frit is on this machine. Each missing source is spelled as the frit spells it. A source that the machine has but refuses is marked `(refused)`. The glyph helper finds the sources under its own sandbox. One helper answers a sources query for every frit, and each answer is waited for at most `HELPER_WAIT`, 5 s. So this process parses no font (ADR-1201 D3). When the helper does not start, fails or stalls, the answers that came are kept. Each frit still unanswered prints `reads` and `unknown:` with why, never `draws nothing`. |
| `check FILE` | any user | A file named `*.vtfrit` is read strictly by the helper's frit reader. The reader prints each warning and `ok FILE`, or fails naming the line. Any other file is checked as a vtium.conf by the console's own `vtium check-config`, both its parts in one run (ADR-1403 D3). The check runs on a copy, in a directory of the tool's own under the temporary directory. Each problem is printed with FILE's own name and line. Writes nothing. |
| `flow`, `diff`, `install`, `get`, `set`, `revert`, `new`, `rule`, `head`, `look`, `draft`, `history`, `restore`, `library`, `edit` | the user; sudo to install | The flow's commands ([The draft](#the-draft)). |
| `new-frit FONT...` | the user; root to install | Makes a frit of the fonts, in order. Each font is a source as a frit spells it (`kbd:ter-v16n`, `builtin:unifont`), or else a family, `font:FAMILY`. `--name NAME` names the frit. Otherwise the name is made from the first font, as lowercase words joined by `-`. `--cell WxH` and `--size POINTS` write `cell=` and `size=`. The frit reader checks the frit before anything is drafted. |
| `new-frit --from FILE`, `new-frit --template` | the user; root to install | `--from` installs the frit in FILE. The frit is named by `--name`, else by its `name=`, else by FILE's name less `.vtfrit`. `--template` prints a frit with each key explained, to edit for `--from`, and writes nothing. `new-frit` alone at a terminal opens the TUI at its new-frit view. Away from a terminal, `new-frit` alone is a usage error. |
| `preview FONT... [--size POINTS]` | any user | Draws each font's specimen with the glyph helper under its own sandbox, as the TUI draws a size's image (ADR-1201 D9). A FONT is a frit's name, a built-in font's name, or a source as `new-frit` takes it. The size is `--size`. Otherwise it is vtium.conf's `font-size`, where that is set and not 0, and otherwise 12 points, at the density that the TUI uses. Each font prints one line, tab-separated: its name, its size, its cell, and how many of the specimen's characters it draws itself. The command asks on `/dev/tty`, in raw mode within a second, whether standard output is a terminal that answers kitty's graphics query before DA1. If it is, the line is followed by the specimen as kitty graphics' raw pixels, with one image id a font and the cursor moved past the image. A font that does not draw prints `failed`, its name and why, and the command exits 1. Writes nothing else. |
| `remove-frit NAME` | the user; root to install | Removes the frit `NAME.vtfrit`. It refuses a built-in font, a name with no frit, and a frit that a `font-name` of vtium.conf names. That `font-name` may be at the top level or in a `[look NAME]` section, and the refusal names its line. `apply` refuses the same removals in any draft. `apply` keeps the removed frit as `NAME.vtfrit.bak`. |
| `use-frit NAME` | the session's user | Switches the running console to a frit or a built-in font (ADR-1200 D5). NAME is checked first, as the console checks it. NAME must be `unifont`, `unscii-8`, or the name of a frit whose `NAME.vtfrit` is a regular file in the frits' directory. When it is neither, the command fails, naming that file. The command then asks the terminal what it is, with XTVERSION and DA1. It sends nothing more to a terminal that does not name vtium. To vtium, it writes `OSC 50 ; NAME ST` and the query `OSC 50 ; ? ST` to `/dev/tty`. It reads the reply in raw mode for at most a second. It drops input left unread, and then puts the terminal's mode back. SIGTERM, SIGHUP, SIGINT and SIGQUIT are held while the terminal is raw, and act once its mode is back. The command prints `set NAME` when the reply names NAME. Otherwise it prints `failed`, the name, and why. The reason is the font that the console kept, no reply because the console's `allow-font-ops` is off, or a terminal that is not vtium. |
| `apply DIR` | root, through `sudo` | Checks the install bundle in DIR and installs it ([The privileged step](#the-privileged-step)). |
| `serve` | the account ADR-1200 D15 names | The helper ([The helper](#the-helper)). The console starts it, and it takes no argument. |
| `report [--save[=PATH]] [--lines N] [--previous-boot]` | any user; root for the parts only root reads | Prints one plain-text document of what a bug report needs, and changes nothing on the machine. Run as a user with `--save`, it writes the document to a new file instead, and prints the file's path. Run as root, it writes no file and refuses `--save`. ADR-1200 D24 lists its parts, the programs it runs, and its privacy rules. `--root DIR` reads the machine under DIR, for the tests. |

Each frit command prints one line a result, tab-separated. It exits 0 on success, 1 on a failure and 2 on a usage error. A frit is a file that no draft of vtium.conf holds, so `new-frit` and `remove-frit` do not go through the draft.

Run as a user, a frit command that changes something writes an install bundle under `$XDG_STATE_HOME/vtium/drafts`. It prints `write PATH` or `remove PATH` for each file that the bundle holds. Then it prints `draft DIR`, the bundle's directory, and `run COMMAND`, the command that installs the bundle. For the system's directories, that command is `sudo /usr/bin/vtium-config apply DIR`. A change that changes nothing prints `unchanged`, and writes no bundle. Run as root, a frit command installs at once. It writes the bundle into a directory of root's own under `/tmp`, applies it, and prints `apply`'s lines.

After the command word, an option that the tool does not take for itself is the command's own, as `new-frit --name` is. The font store's commands, `create`, `list`, `verify` and `remove`, are gone with the store (ADR-1200 D3, D12, retired). Each of them is now an unknown command. `install` is the flow's command now.

The frits are in the directory that `font-frits` names. Where `font-frits` names none, the frits are in `/etc/vtium/fonts`, which `VTIUM_FONT_FRITS` changes at build time. vtium-config reads `font-frits` from the top level of the one file that the console reads: `vtium.conf` in its configuration directory (ADR-1403 D2). `fonts`, `new-frit`, `remove-frit`, `use-frit`, `preview`, the TUI and `apply` list, write and remove frits in that directory.

vtium-config does not see a `font-frits` that the console takes from its own command line, as the VT review's `run-vt.sh` passes it. For that case, `--frits DIR` names the directory. vtium-config takes `font-frits` only as an absolute path, as the console does. The console ignores a relative or empty `font-frits`, and says why. vtium-config makes each of those commands fail instead, naming the file and the key. It does the same for a configuration file that it cannot read. `get` and `set` still read and set vtium.conf's key in the draft, so that the user can mend a vtium.conf. `set` refuses a value that is not an absolute path.

vtium-config refuses an install or an install bundle that changes vtium.conf's `font-frits` and also writes or removes a frit. Its frits would go to the directory that `font-frits` named before. So the user installs the new directory first.

These options change the paths and programs that vtium-config uses:

- `--frits DIR` names another directory in place of `font-frits`.
- `--configdir DIR` names another directory than `/etc/vtium` for vtium.conf.
- `--sudo PROGRAM` replaces `sudo` in an install's privileged step.
- `--vtium PROGRAM` names the console binary that `apply` checks files with. It is `/usr/bin/vtium`, unless `VTIUM_PROGRAM` set another at build time.
- `--kbd-dir DIR` (repeatable), `--font-dir DIR` (repeatable), `--vconsole-conf FILE` and `--kernel-cmdline FILE` replace the paths that `serve` finds sources in.
- `--sys DIR` replaces `/sys`, where the TUI and `preview` measure the displays, and where the flow's commands read the devices present.
- `--udev-data DIR` replaces `/run/udev/data`, udev's database. Its directory's `control` names the daemon's socket.
- `--helper PROGRAM` replaces the glyph helper that the tool starts, this binary's `serve`, as a test's stand-in does.

vtium-config takes none of `--frits`, `--configdir`, `--sudo` and `--vtium` as root ([The privileged step](#the-privileged-step)).

`vtium-config --help about` draws a banner of its name in glass (`src/banner.rs`, vtium/vtium#112), and says what the program is. `--help` lists `about` among its help topics. `--help COMMAND` prints one command's own help, from the text that the usage reads (`src/help.rs`). The banner image is drawn only when stdin and stdout are both terminals. It is never drawn with a non-empty `NO_COLOR`, or in a Chinese, Japanese or Korean locale. It is never drawn into a pipe, where `--about` prints plain text. No other command prints the banner. `--no-banner` does nothing now. vtium-config accepts it so that scripts that pass it keep working.

## The TUI

At start, the TUI writes four requests at once: XTVERSION, the OSC 50 query, kitty's graphics query and DA1. It waits at most a second for DA1's reply, which every terminal of the VT100 line sends after the others. In a vtium console that answers the OSC 50 query, each font that the user highlights is sent as `OSC 50 ; NAME ST` and the query. The TUI sends one switch at a time. The next switch goes when the last one's reply comes, and it names whatever is highlighted then. A reply that names another font marks the font as kept by the console. With `allow-font-ops` off, the TUI says so, and offers to turn it on as a change in the draft. In any other terminal, the TUI sends no OSC 50 apart from the query at start.

The user can leave the TUI by `q`, by Ctrl-C, or by SIGINT, SIGTERM, SIGQUIT or SIGHUP. Each of these switches the console back to the font that the start query named, and waits for the reply. A panic writes the switch back from the panic hook, since a release build aborts on a panic. Nothing can give the font back after SIGKILL, or once the terminal is gone.

In a vtium console that answers the OSC 50 query, the Fonts and Looks views show no image, and they ask the glyph helper for none (`src/tui/console.rs`, ADR-1201 D4). The Fonts view prints the detailed specimen's lines below the list, with its line of superscripts and subscripts as SGR 73 and 74. The Looks view prints the sample screen of a size's image, in its colors. Both views take their lines from the code that draws the images, `specimen::detail_lines` and `shot::sample_rows`. The console draws those lines in the font that it draws now, so `t` shows the tried font in them. The Looks view's caption says that a size takes effect when vtium starts on the console again.

In that console, `s` in the Fonts view saves the font that the console shows into the draft. It writes the font where the console's head takes its font from. The head is the one that `VTIUM_HEAD` names, which vtium sets in each console's session. When that head's look names a font of its own, the font goes into the look, through the Flow view's plan of a path's changes. Otherwise the font goes into the top-level `font-name`. Without `VTIUM_HEAD`, when a look names a font, the TUI saves nothing and says why. The draft is written only when `vtium check-config` passes it, or fails it only for a missing complete route. Otherwise the message line shows what the check printed.

Outside a vtium console, and in one with `allow-font-ops` off, the preview is the image or the written description. There `s` switches the Fonts view's image.

The TUI's views are Flow, Looks, Fonts, Settings and Help, in the order that Tab steps through them. `vtium-config tui` opens the TUI on Flow. Tab moves to the next view and Shift-Tab to the one before, wherever the selection is. The arrow keys move inside a view (#355). Some things, while open in a view, would be left behind by a change of view. While one of them is open, Tab and Shift-Tab do nothing, and a click on a view's name does nothing either. These things are a box's detail, a part maker or a picker in the Flow view, the file view, Restore's list, a setting's value being typed, and the new-frit view. Esc closes each of them. A question takes Tab as no, like any key apart from its yes.

Flow draws the configuration as paths (ADR-1403 D7). It is one document that scrolls, in three sections:

- CONFIGURATION shows each path of the draft as a row of boxes, joined by lines. The row holds the path's displays, then its terminal, then the inputs that type into it.
- NEW PATH is a path being built.
- PARTS lists the displays, terminals, inputs and font sets to build a path from. Each group is led by its template.

The boxes reflow to the terminal's width. The arrow keys move the highlight from box to box. Space picks a part into NEW PATH. `a` adds the path in NEW PATH to the configuration. `e` takes a path out of CONFIGURATION into NEW PATH, to revise it. Esc clears NEW PATH.

Enter opens a box. For a part, it opens the part's detail with its label. For a template, it opens the template's part maker and its pickers. For a terminal, it opens the terminal's card. The card changes the terminal's name, font set, size and settings. The card also draws the terminal's subflow: the font set, then what its look draws, then its displays. `v` shows the draft as vtium.conf's text, read only, with the unsaved changes applied. `s` and `i` save and install from there too.

The Flow view alone turns on mouse reporting, modes 1000 and 1006. A click highlights a box, and a second click opens it. A click on a view's name switches to that view. With `NO_COLOR` set, or with a `TERM` that is unset or `dumb`, the Flow view draws no color. Its borders and words still tell each state apart. It draws the current draft ([The draft](#the-draft)), with the devices present placed on it. It follows udev's device events while it is open, through vtium-platform's watch, which opens no device node. `d` reads every device again.

Every change that the TUI makes goes through `src/ops.rs`. The change waits with the TUI's other unsaved changes until Save, on `s`, writes them into the draft. After each change, the check runs on the draft with the changes applied. The status line counts three things: the draft against the installed file, the changes not saved, and the check's problems. Leaving by `q` while changes are not saved asks first. `y` saves the changes, `n` leaves without them, and Esc goes back. Ctrl-C and the signals above save the changes before the TUI leaves. Previewing a whole path needs the console's control socket, which vtium does not have yet, so `p` says so.

Looks holds the console's sizes until looks are built. Its preview draws a screen of 1024 by 768 pixels (`PREVIEW_SCREEN` in `src/tui/shot.rs`), filled with fixed sample content. It draws the screen at the character cell that the highlighted size gives on the display, whatever the display's own size. The sentence beside the preview counts the characters and lines that the console has on the real display. Fonts lists the frits and the built-in fonts, and `n` opens the new-frit view. Settings lists vtium.conf's keys. Help holds the keys of each view and the commands. Its About page holds what `vtium-config --help about` prints.

The tab bar shows the actions of the view in front:

- beside Flow: View file, Save, Install, Revert and New, with Restore on `o`;
- beside Looks, Fonts and Settings: Save and Install;
- beside Help: none.

Install, on `i`, saves first. Then it installs through the same three steps as `vtium config install` (`Session::prepare`, `put` and `finish` in `src/drafts.rs`). It refuses a draft made before the installed file changed, and a draft that the check fails. It asks `Install the draft?`, with the lines added and removed and the frits waiting. Revert, Restore and New each ask first too, and count the unsaved changes that they throw away. Install gives the terminal to sudo for the privileged step.

In the Fonts view, `r` marks the highlighted frit to be removed when the user installs, and a second `r` keeps it. The TUI refuses what `remove-frit` refuses, by the same check (`removal_refusal` in `src/commands.rs`). It refuses a built-in font. It refuses a frit that a `font-name` of vtium.conf names, at its top level or in a look. It checks both vtium.conf as it is and vtium.conf as the install leaves it. It also refuses a frit that a new frit in the same install replaces. A frit is a file that no draft holds. So a new frit and a frit's removal wait in the TUI, and go into the next Install's bundle, before vtium.conf. Leaving asks before it drops them. The font list marks a frit to be removed, and `apply` keeps the old file as `NAME.vtfrit.bak`.

The settings view lists every key that vtium.conf takes, from the console's own key table in vtium-settings. The font keys come first: `font-name`, `font-size`, `font-frits`, `font-hinting`, `font-line-spacing`, `allow-font-ops`, `display-dpi`, and the sprite policy's `glyph-source` keys. The rest follow in the table's order. Each key shows its value or its default. Under the list, the view says what the highlighted key does. Enter turns a switch on or off. On a key of a fixed set of words, Enter steps to the next word. On any other key, Enter starts typing the key's value, and a second Enter writes the value into the draft. An empty value leaves the key at its default. Moving away drops what was typed.

The new-frit view is `new`'s guided flow (ADR-1201 D9). The user types the frit's name, up to three sources, a cell and a size, and Enter moves to the next field. Enter on the last row adds the frit to the next Install, once the frit reader takes it. A frit with an installed frit's name replaces the installed frit, and Install replaces the text that the TUI read at start. An installed frit that is not UTF-8 text is not replaced, and the view says to remove it first. While a field or a value is typed, every character is text, `q` and `i` included. Ctrl-C still leaves.

Sometimes the terminal answered kitty's graphics query, but the console cannot be switched live. That happens when `allow-font-ops` is off, or when the terminal is not vtium. Then the Fonts view shows the highlighted font's specimen as an image below the list. The image is at the draft's `font-size`, else at 12 points. The view deletes the image as the highlight moves. A frit that does not read gets no image.

Where the terminal answered kitty's graphics query, each size shows a specimen image. The image is at the pixel height that the size gives, at the density that ADR-1200 D17 orders:

1. vtium.conf's `display-dpi`;
2. else the density down the screen of the first connected display whose EDID states its size, in the order of the connector's name (`src/display.rs`);
3. else 96 dots per inch.

`src/display.rs` reads each connector's `status`, its first mode and its EDID's base block from sysfs, and opens no card. The TUI says which density it used.

The specimen is three lines of Latin, digits and box drawing. A fourth line is in the locale's letters, where `LC_ALL`, else `LC_CTYPE`, else `LANG` names a language whose script takes a cell a character. Those are Cyrillic, Greek, Japanese, Chinese, Korean, and the accented Latin of German, French, Spanish, Portuguese, Polish, Czech and Slovak, Turkish and Vietnamese. A script whose letters join or stack has no line until the specimen shapes clusters. Arabic, Hebrew's points, Thai and Devanagari are such scripts. A CJK character takes two cells, as the console draws it. Where the terminal draws no images, a size shows a sentence instead. The sentence gives the cell that the size gives, the grid that cell gives on that first display, and how many of the specimen's characters the font draws itself.

Once the terminal is known, the TUI asks one helper which sources each frit that reads lacks. It uses the sources query that `fonts` uses, on a thread of its own whose answer wakes the loop. So the TUI goes on reading keys, Ctrl-C included, while the helper works. The font list notes a frit that lacks some sources, as `reads, 1 source missing`. It also notes a frit none of whose sources is on this machine. A frit that the helper gives no answer for in its wait, as when the helper stalls, is noted `reads, sources not checked`. The answers that came for the other frits are kept. The helper draws the glyphs under its own sandbox ([The helper](#the-helper)), so the TUI's process parses no font. The image goes as kitty graphics' raw pixels, `a=T,f=32,t=d`, in chunks. It is deleted with `a=d` when the highlight moves or the TUI leaves.

The TUI needs a terminal of at least 80 by 25 cells (`MIN_COLS` and `MIN_ROWS` in `src/tui/view.rs`; ADR-1201 D3). A large previewed font can leave the terminal smaller than that. Below that size, the TUI draws no view. Its one screen says the size it needs and the terminal's size, and lists the commands, which work at any size. The screen keeps two keys: `q`, which leaves, and `u`, which puts the console's start font back. The TUI's sentences and keys wrap by words. Every view shows its tab bar, status line, message and keys whole. The words that the TUI shows are in `src/tui/words.rs`.

## Reviewing the screens

The maintainer reviews the TUI's screen goldens in `tests/screens/` as images and by using the TUI, not as text. Two make targets serve that review, and neither is a gate.

`make tui-shots OUT=DIR` draws every screen golden as a PNG, `DIR/NAME-COLSxROWS.png`. It draws the accepted goldens and the provisional ones alike. Without OUT, the PNGs go to `target/tui-shots`. The target runs the two golden tests of `tests/tui.rs` with `VTIUM_CONFIG_SHOTS=DIR/bytes`. Under that variable, each test also writes two things for each golden: every byte that the TUI wrote to reach the screen, and the screen as text. The tool `tui-shots` in `tools/tui-review/` then plays each byte stream through vtium-term and vtium-render in unifont, 8 by 16 pixels a cell, as tier 2 draws its screens. So each PNG shows what a vtium console in unifont shows for those bytes.

Before `tui-shots` writes a PNG, it checks that the screen the test read is the committed golden in `tests/screens/`. It also checks that the same bytes, played into a terminal of its own, read as that screen. So each PNG shows its committed golden. A golden that has drifted, as a provisional one can without failing its test, is named and not drawn, and `make tui-shots` fails. The bytes come from the golden tests rather than from the binary on a pty, because the goldens' fixtures reach the TUI through its loop in the test process. The binary writes its screens through the same loop.

`make tui-demo` opens the real binary's TUI in your own terminal, on the fixtures of the goldens named `flow-*`. The fixtures are the draft `bench` of their installed vtium.conf, and their stand-in console as the check. They also hold the keyboard `kbd` at `pci-0:3.3`, laid out as a sysfs tree and a udev database. `make tui-demo FIXTURE=figure` opens the TUI on the flow of ADR-1403 D7's figure. That flow comes from no draft, so in that demo the view changes nothing.

`tools/tui-review/demo` makes a throwaway directory, and points `HOME` and every XDG directory into it. It removes the directory when the TUI leaves. The tool `tui-demo` refuses to start unless that directory meets these conditions:

- It is fresh and empty, and named by its real path.
- It is neither your home nor a directory above your home.
- `HOME` and every XDG directory lie under it, with no `..` in their paths.

`tui-demo` also refuses to run as root. vtium.conf and the frits live in the throwaway directory too. So Install runs `vtium-config apply` as you, into that directory, and never asks for sudo. The sudo that the TUI is given refuses to run. Nothing that the demo does reaches your own `~/.config`, `~/.local` or `/etc`.

## The draft

The flow's commands and the TUI change the user's draft. The draft is a whole vtium.conf, kept in vtium-config's store. Only `install` writes `/etc/vtium/vtium.conf` (ADR-1403 D8). vtium reads only the installed file, so nothing that the user edits reaches the console until it is installed.

vtium reads vtium.conf only when it starts on a VT. A head whose `vt` names VT N appears only where vtium runs on VT N. So after an install writes `/etc/vtium/vtium.conf`, the TUI's Install and `install` both run the same step: `Session::after` in `src/drafts.rs`, and `src/vts.rs` (ADR-1201 D5).

The step reads `vtium console status` as the user. For each VT that a head names where vtium is not enabled, it runs `vtium console enable --direct N`, or `--seat N`, through sudo. It uses the mode that the VTs vtium is enabled on use, else direct mode. It shows what `vtium console` prints, refusals included. A refusal leaves the install in place. The step then asks whether to start vtium on each VT that it enabled, and to restart vtium where vtium runs. Only a yes runs `systemctl start` and `systemctl restart` through sudo. The TUI asks on the terminal that it gives back for sudo. `install` asks only at a terminal. Without a terminal, and for a command's `--install`, the step prints the commands instead. An install into other directories, through `--configdir`, runs no such step, because no vtium on a VT reads that vtium.conf.

| Path | What it holds |
|---|---|
| `$XDG_CONFIG_HOME/vtium/drafts/NAME.conf` | Each draft. The first draft is made from the installed file. When none is installed, it is made from the six-line comment `HEADER` in `src/drafts.rs` alone. The draft is named for the machine's hostname, with `-2`, `-3` and so on appended, so that no draft is overwritten. |
| `$XDG_CONFIG_HOME/vtium/vtium-config.conf` | `draft = NAME`, the current draft, and `history = N`, how many installed files the history keeps, 10 unless set. |
| `$XDG_CONFIG_HOME/vtium/library.conf` | The library, in vtium.conf's section syntax. |
| `$XDG_CONFIG_HOME/vtium/installed/DRAFT-N.conf` | Each installed file that an install of DRAFT replaced. |
| `$XDG_STATE_HOME/vtium/bases/NAME.base` | Each draft's base: the SHA-256 of the installed file that the draft was made from or installed as, or `none`. |
| `$XDG_STATE_HOME/vtium/drafts/` | The install bundles that `install` hands to `apply DIR`. |

`XDG_CONFIG_HOME` is `~/.config`, and `XDG_STATE_HOME` is `~/.local/state`, when they are unset. Run as root, vtium-config finds root's home from the password database. It never takes root's home from `$HOME` or the XDG variables, which `sudo` can carry over from the user who ran it. So a root draft lands only in root's own home.

Every command that changes the draft takes these steps:

1. It reads the draft afresh.
2. It makes its change through `src/ops.rs`.
3. It writes the draft, whatever the check says, so a draft may be unfinished.
4. It runs `vtium check-config` on the draft.
5. It prints each line that the check prints, and then the state line, `draft NAME: 3 lines added, 1 removed · 1 problem`.

The state says `same as installed`, or the lines added and removed. It says `made before the installed file changed` once the installed file's hash is no longer the base. With `--dry-run`, a command prints the change as a difference and the check's verdict, and writes nothing. With `--install`, the command then installs the draft.

`install` refuses a draft made before the installed file changed. Next it refuses a draft that the check fails. Then it builds an install bundle of the draft and runs `sudo vtium-config apply DIR`. As root, it installs at once instead. After a good install, it saves the file that it replaced in the installed history. It makes the draft's base the new installed file.

`edit` copies the draft to a file under `$XDG_STATE_HOME/vtium/`. It opens that file in `$VISUAL`, else `$EDITOR`, else `vi`. It makes the edited copy the draft. When the check fails, it asks whether to edit again. With `--install`, it installs only a draft that the check passes.

A rule's number is its place in `[rules]`, counted from 1. `rule add` without `--before` places a rule through vtium-settings's `placement`. A rule that names one device goes before the first rule of its kind that names a kind of device. A rule that names a kind goes before the first `*` rule of its kind. `flow` prints the draft's flow, with the devices present placed on it. It reads the devices from sysfs and udev's database, without privilege. `flow --json` prints one object whose `version` is 1, with the draft's name, state and problems.

The exit codes 0 to 5 are `src/outcome.rs`'s, and [Configure vtium](../../docs/configure.md#the-commands) lists what each means.

`head rm` removes a head, and leaves the rules that send to it. It names those rules, and the check lists them until each is pointed elsewhere or removed. `look rm` refuses a look that a head names, and names the heads. With `--force`, it first sets those heads to the default look. `--name` on `head set` and `look set` renames the section, together with every rule or head that names it, in one change. `head active` needs the control socket that ADR-1401 D12 plans, and says so with exit 2.

A debug build run under cargo, as every test is, refuses the real user's store with exit 5. That store is `~/.config/vtium` and `~/.local/state/vtium`, under the home that the password database gives. Every test that runs the binary starts it through `tests/common/home.rs`. That file points `HOME`, `XDG_CONFIG_HOME` and `XDG_STATE_HOME` at a directory of the tests' own. `tests/cli.rs` holds a test that forgets to on purpose, and is refused. A release build, the one that users install, never refuses.

Every path that the store builds from a draft's name goes through one check, `store::named`. The check refuses, with exit 2, a name that is not 1 to 64 letters, digits, `.`, `_`, `+` and `-`. It also refuses a name that starts with `.` or `-`. vtium-config records a draft's base the first time it sees the draft. So a draft copied in from another machine is refused at install, once the installed file changes after that.

## The privileged step

An install becomes an install bundle. The bundle is a directory that the user owns under `$XDG_STATE_HOME/vtium/drafts`, named for the time it was made. It holds the whole new vtium.conf, each frit to write under `fonts/`, and a `MANIFEST`. The `MANIFEST` names each file with the SHA-256 of the installed file it replaces, or with `new`.

When the directories are the system's, vtium-config writes the bundle and runs exactly one command. It builds that command as an argv, and never as a shell string: `sudo /usr/bin/vtium-config apply BUNDLE`, with no option. So a sudo rule can name the command whole. `vtium config install` runs the command as a plain child, with the terminal as its standard input, so that sudo can ask for the password.

The TUI leaves its alternate screen while sudo asks. It runs the command in a process group of its own, in the terminal's foreground, so that Ctrl-C reaches sudo and not the TUI. It does this through `--run-with-signals`, which releases the signals that the TUI holds, in the child alone. When the command fails, the TUI shows what the command printed. The TUI runs as the user from start to end (ADR-1201 D5). Run as root, the TUI does not open. It says to run `vtium config` as yourself, since it asks for sudo when it installs.

A command run as root installs at once, with no sudo. It writes the bundle into a directory of root's own under `/tmp`, and applies it. `--configdir`, `--frits` or `--vtium` may name anything but the system's directories and console, as the tests do. Then vtium-config offers no privilege at all. It runs `vtium-config` with those options and `apply BUNDLE` as the user.

**Which tools can run apply.** Run as root, apply takes the user that an install bundle belongs to from one of three variables:

- `SUDO_UID`, which sudo sets;
- `PKEXEC_UID`, which pkexec sets;
- `DOAS_USER`, the user's name, which doas sets and apply looks up in the user database.

So `sudo`, `pkexec` and `doas` each work as `--sudo PROGRAM`. A tool that sets none of them can install only a bundle that root owns. Each tool sets only its own variable. So when more than one is set, one was kept from the user's environment or forged, and apply refuses the bundle with exit 1 and says so. For example, doas with `permit keepenv` keeps a `SUDO_UID` that the user set, which would otherwise name another user's bundle. Each variable is still only as trustworthy as the tool that sets it. A tool that lets the user set its own variable can make apply read a bundle of any user that the variable names.

**What `sudo vtium-config apply` can change.** Run as root, vtium-config refuses every option that names a path or a program, each with exit 2 and the reason. These options are `--configdir`, `--frits`, `--sudo`, `--vtium`, `--kbd-dir`, `--font-dir`, `--vconsole-conf`, `--kernel-cmdline`, `--sys`, `--udev-data`, `--helper` and `--root`. It also refuses `--run-with-signals` in the same way.

Run as root, vtium-config also refuses the commands that run as the user, with exit 2 and the words to run them as yourself:

- `check` and `new-frit --from` read a file that the user names. As root they would read any file, and could print what is in it.
- `fonts` and `preview` start the glyph helper, which refuses root.

`new-frit FONT...` and `remove-frit` run as root. So do the flow's commands, on root's own draft. They write only text that they build themselves, through `apply`'s steps. `report` runs as root to read the kernel's display state and the whole journal. It writes no file, and refuses `--save` in any form. So a rule that lets a user run the report as root never lets that user create a file where root can. The user's own shell saves what the report prints, as in `sudo vtium-config report > vtium-report.txt`.

So `sudo vtium-config apply BUNDLE` writes only two kinds of file. It writes `/etc/vtium/vtium.conf`. It writes frits in the frits' directory that `/etc/vtium/vtium.conf`'s `font-frits` names, else in the one that the build names. It writes each file only after the frit reader or the console's own `vtium check-config` has accepted the file. It runs no program but the console that the build names, which is `/usr/bin/vtium` unless `VTIUM_PROGRAM` set another.

vtium-settings's `edit_conf` edits vtium.conf's top level line by line. It reads each line with `key_line`, the console's own reading. vtium-settings's section writer edits vtium.conf's sections. So every comment, blank line and line ending stays (ADR-1403 D8).

`apply BUNDLE` takes these steps:

1. It reads the bundle without following a link. It takes only regular files with names it knows and no second name (a hard link). Each file must be owned like the bundle's directory, by root or by the user that the privileged tool ran it for. It bounds each file, since root is reading a directory that the user owns. Root read those files for the user, so what apply says of them names a file, a line and a key, and quotes nothing they hold.
2. It refuses the bundle, writing nothing, when a file that the bundle replaces has changed since the bundle was made. It names the file.
3. It checks every file before it writes any. It checks each frit with the helper's frit reader. It checks vtium.conf, its top level and its sections in one run, with the console's own `vtium check-config`. The check runs on root's copies of exactly the bytes that apply then writes. Apply makes those copies in a new directory of its own under `/tmp`, whatever `TMPDIR` says, with a random name and mode 0700. It creates each copy through the directory's descriptor, without following a link.
4. It writes the frits, then vtium.conf, so that vtium.conf never names a frit that is not there. It writes each file through vtium-settings's `install_file`. That function writes a new file beside the old one, with mode 0644, flushes it and renames it over the old one. The old file is kept as `NAME.bak`.
5. It prints one line a file: `wrote PATH`, `removed PATH`, `failed PATH` with the reason, or `not written PATH` after a failure. It exits 0 when it wrote them all. The bundle stays, so it can be applied again.

The step is one short process. Most of its time goes to the one run of `vtium check-config` in step 3. That run reads the devices present for the complete-route check, as the user's own check did.

## Frits

A frit has one `key=value` a line. Blanks around the key and the value are dropped. A `#` starts a comment that runs to the line's end, as vtium.conf reads it. Unlike vtium.conf, a frit is refused whole for any of four faults, and the refusal names the file and line. The faults are an unknown key, a malformed line, a value with a control character, and a key that this build does not build yet.

This slice reads ADR-1200 D1's keys and ADR-1301 D4's `antialias=`:

- `name=`;
- `source=` (`font:`, `kbd:`, `file:`, `bdf:`, `outline:`, `vconsole`, `console:VT`, `builtin:`);
- `unimap=`;
- `script-source=`;
- `cover=` (`ascii`, `latin-1`, `gb2312`, `gbk`, `big5`, `jis-x-0208`, `ks-x-1001`, `U+XXXX` and `U+XXXX-U+YYYY`);
- `cp-size=`;
- `style=`, which is `regular`, `bold`, `italic` or `bold-italic`, for the source above it. The style is regular when no line names one. Any other value refuses the frit, naming its line.
- `cell=`, `size=`, `glyph-source=` and `glyph-source-<range>=`, `antialias=`, `hinting=` and `license=`.

The store's `package=` and `sha256=` are unknown keys. `compress=` chose how a stored set was compressed. vtium-config accepts it with one warning, naming the file and line. Refusing it would leave the console on the built-in font, for a key that never changed what was drawn.

`cell=WIDTHxHEIGHT` states the cell that the console draws the frit in, and that the helper generates for (#145). Each side is 6 to 96 pixels. The console reads `cell=` as text before its first frame.

`size=`, given once, is an outline frit's starting size in points, from 6 to 96. A zoom steps from that size. The console reads `size=`, and converts it at the display's density (`crates/vtium/README.md`). The helper draws at the cell that each request names.

`glyph-source=`, and `glyph-source-box=`, `-block=`, `-braille=`, `-sextants=`, `-octants=` and `-legacy=`, take `font`, `sprites` or `auto`. These are the configuration's names and values, and each may be given once. The console reads them as the frit's sprite policy, with its configuration's keys over them.

`antialias=` is `auto` or `no`. The engine reads it once ADR-1301 D6's grid is built.

`hinting=`, given once, is `light` or `full`. `light` is the default, and fits outline glyphs to the pixel grid up and down only. `full` also fits them sideways. See [Outline fonts](#outline-fonts) for how.

`script-source=` names a smaller size of the same font as the bitmap `source=` line above it, as `kbd:NAME`, `file:PATH` or `bdf:PATH`. The line can repeat (ADR-1202 D12). The source's superscripts and subscripts (SGR 73 and 74) come from one of its smaller sizes (ADR-1202 D3). It is the tallest smaller size whose characters are from one half to three quarters as high as the source's characters in the cell. Those smaller sizes are these, each at any whole multiple that fits the cell:

- the source itself at a smaller whole multiple, when it is drawn at 2x or more;
- a CPI source's other heights;
- the fonts that the `script-source=` lines name.

The glyph is placed whole, against the top of the row for a superscript and against the bottom for a subscript. A font that this machine lacks is left out quietly. A font that the helper refuses is left out with a warning naming its line. The font keeps its own Unicode table, and takes the source's `unimap=` where it has none. The line under an `outline:` source refuses the frit, naming its line. The line under a `font:` source that turns out to be an outline font is ignored, with a warning. `vtium-config check` asks the helper about a frit with `script-source=` lines. It notes each font whose height gives no superscript or subscript in the frit's own cell. For example, a frit of Terminus at 16 pixels takes its superscripts from Terminus at 12:

```ini
source=kbd:ter-116n
script-source=kbd:ter-112n
```

`line-spacing=`, given once, is a whole percentage from 0 to 100 of the text's height, which the console adds to each row (ADR-1302 D13). The console reads it as text, as it reads `cell=`. vtium.conf's `font-line-spacing` overrides it. `fit=` says how the set on the `source=` or `cover=` line above it uses the taller row. It is `fill`, `natural` or `room`, once for each set. A `cover=` line's `fit=` wins over its source's. A character in no set with a `fit=` takes its default. Line and block characters fill. Arabic, Hebrew, Syriac, Thaana, N'Ko and combining marks have room. The rest is natural.

A source's `cover=` lines are one union, sorted and merged. `ascii` is U+0020 to U+007E, and `latin-1` adds U+00A0 to U+00FF. Each named set is the codepoints that a Unicode mapping file maps to. `gb2312`, `big5`, `jis-x-0208` and `ks-x-1001` come from the standards' own tables, with no vendor's extension. `gbk` comes from Microsoft's code page 936, whose table also maps U+0000 to U+007F and, at 0x80, U+20AC.

The files lie under unicode.org's `Public/MAPPINGS/`:

- `gb2312` from `OBSOLETE/EASTASIA/GB/GB2312.TXT` (7,445 codepoints);
- `gbk` from `VENDORS/MICSFT/WINDOWS/CP936.TXT` (21,920);
- `big5` from `OBSOLETE/EASTASIA/OTHER/BIG5.TXT` (13,704);
- `jis-x-0208` from `OBSOLETE/EASTASIA/JIS/JIS0208.TXT` (6,879);
- `ks-x-1001` from `OBSOLETE/EASTASIA/KSC/KSX1001.TXT` (8,224).

unicode.org no longer serves `GB2312.TXT`. So it is taken from the Wayback Machine's latest capture of its original URL, of 2004-06-18, and pinned by its SHA-256 as the others are. `KSC5601.TXT` beside `KSX1001.TXT` is Unified Hangul, Microsoft's extension, and is not used.

`tools/cover-tables` fetches each file once with curl into `$XDG_CACHE_HOME/vtium/cover-tables`. It writes `src/cover.rs`, its SHA-256 in `src/cover.rs.sha256`, and `tools/cover-tables/provenance.tsv`. The provenance file pins each file by its URL, its own table version and date, its SHA-256, and its count of codepoints. `cargo run -p vtium-cover-tables -- generate` writes them. It refuses a file whose SHA-256 moved, unless it is given `--repin`. `-- check` fails when the checked-in files differ from what the cached files generate. Nothing fetches at build or run time, and the helper reads only the checked-in tables. A test pins each set's count. Another test holds `src/cover.rs` to `src/cover.rs.sha256`, so a hand edit that keeps every count still fails.

A frit is meant to travel, so a source names a font that each machine finds for itself. A source names it in the `font:` form below.

```ini
source=font:Terminus 16      # a family, matched on the machine that draws it
style=regular
```

`font:FAMILY` matches as fontconfig matches a family, with case and blanks ignored. kbd console fonts match it first, by name. The match is the font that `setfont` would find by that name. Otherwise it is the first font, in each data directory's sorted `consolefonts/`, whose name less its suffixes is the family. A PSF file outside kbd's directories carries no family in its header, so only `file:` names it.

With no kbd font of that name, an installed outline font matches. The font is a `.ttf`, `.otf`, `.ttc` or `.otc` file under the font directories, with a face whose name table gives that family. The font directories are `/usr/share/fonts` and `/usr/local/share/fonts`, or each `--font-dir`. The family may be typographic (name ID 16) or legacy (name ID 1), with case, blanks, `-` and `_` ignored (ADR-1500 D8). So `font:JetBrains Mono` finds `JetBrainsMono-Regular.ttf`, whatever a distribution names the file. It also finds a variable font such as `JetBrainsMono[wght].ttf`.

Reading a name table costs a file open, so the search takes two passes:

1. The first pass takes the files whose name less its extension is the family, or the family and `Regular`. It takes the plain name first, then the font directories in order, then the files by path. It keeps the first file whose name table confirms the family, whatever the style of its face.
2. Only when no file passes the first pass does the second pass run. It reads the name table of every other file, in that same order. It takes the first file with a face of the family whose subfamily is `Regular`, with case ignored. The subfamily is the typographic subfamily (name ID 17), or else the subfamily (name ID 2). Failing that, it takes the first file with any face of the family.

A family's name counts in every language that the table holds. The search reads each file's table directory and `name` table alone. It reads them through `names::faces` and read-fonts, the table reader under swash. It opens at most `FAMILY_FILES_MAX` files. A search that would open more fails, naming the family and the cap. When it found a file of another style, it names that file too. The engine chooses the face that it draws with the same reader and the same test, `names::matches`. So the engine takes every face that the search finds. In a collection, it takes the family's first Regular face, and else its first face. `outline:PATH` names an outline file by its absolute path, and `bdf:PATH` names a BDF font.

A source that is missing on this machine, or that cannot be read, is left out. The next source in the frit's order then gives its glyphs. A source that is on this machine and is refused, such as a BDF file that the reader refuses, is also a warning. The warning names the frit's line and why, and the helper sends it to the console to log. A missing source is left out quietly, since a frit names fonts that a machine may not install. With every source left out, the frit fails, naming each source and why.

Each later source fills only the codepoints that the earlier ones lack. A bitmap font is scaled by the largest whole factor at which its cell fits the cell asked for. It is centred across, and placed on the cell's bottom row. A bitmap font whose cell does not fit at factor 1 fills nothing. The cell holds one strike for each pixel format. Bitmap fonts give 1-bit glyphs, outline fonts give R8 coverage, and color glyphs give premultiplied RGBA. Each strike is stored raw, and the strikes' codepoints do not overlap.

## Outline fonts

The engine rasterises each outline font with swash at the cell asked for, so the console's grid never moves for a font. The engine places each glyph in its cell by the rules below.

**Size.** The frit's first outline source is sized so that its ascent and descent span the cell's height less half a pixel. A FreeType cell request for that height does the same. Each outline source after the first draws at the first source's size, as many pixels to the em. So Noto Sans CJK's ideographs behind DejaVu Sans Mono draw at DejaVu's size.

A glyph of such a later source may have ink that would pass its cells, as a large Nerd Font icon's ink would. Such a glyph is drawn smaller until it fits. It is centred across its cells, and on its baseline as far as the row allows, as a color glyph is. A letter of a joining script from such a source is not drawn smaller. It uses the reach as the first source's letters do. A shaped cluster keeps the mark rule (ADR-1200 D17, ADR-1306 D12). A source behind a bitmap first source is sized by its own ascent and descent.

**Baseline.** The baseline sits the ascent, rounded up, below the top. It is raised by the whole pixels by which the lowest ink of the font's printable ASCII, U+0021 to U+007E, drawn at that cell, passes the cell's bottom edge. So the underscore and the descenders end in the cell's bottom rows (ADR-1302 D3).

**Width.** The font that sets the cell sets the cell's width by the W rule (ADR-1200 D17). The width is the wider of `W`'s advance, rounded, and the columns that `W`'s ink covers. For a font with no `W`, the rule uses `M` or `0`. For a font with none of them, the width is half the cell's height. So the cell's width never comes from the font's missing-glyph box.

An `outline:` source that holds only bitmap strikes is sized the same way, at the height that the console names, by its `W`'s advance. Its color strikes are scaled to the cell. The helper reads pixel fonts only from BDF and the kbd formats, never through swash (ADR-1200 D14).

**Hinting.** The ink is measured on the outline unhinted, whatever the frit's `hinting=` says. Light hinting moves only rows in a TrueType font. So the unhinted measure gives the columns that the default hinting draws. For example, DejaVu Sans Mono's `W` at 16 pixels covers 9 columns, where full hinting would snap it to 8. Glyphs are drawn hinted as the frit's `hinting=` says (ADR-1200 D1). Only the W measure that sets the cell is unhinted.

With `light`, the default, the rasteriser fits each glyph to the pixel grid up and down only, as FreeType's light load (`FT_LOAD_TARGET_LIGHT`) does. It hints a TrueType font with skrifa's automatic hinter, which moves only rows. It hints a CFF font by the font's own hints, through skrifa's CFF hinter, which move only rows too. FreeType makes the same choice for a light load.

With `full`, the rasteriser runs a TrueType font's own instructions. For a TrueType font that has none, it runs the automatic hinter at full strength. So a glyph can also move sideways onto whole columns. A CFF font's hints move rows only under either setting, as FreeType's CFF hinter's hints do. So `hinting=full` changes nothing for a CFF font. For example, Nimbus Mono PS and Source Sans 3 draw byte for byte the same both ways.

For DejaVu Sans Mono, JetBrains Mono and Liberation Mono at 12 to 32 pixels, the rasteriser draws every printable ASCII glyph as swash's own scaler hinted it before `hinting=` existed. swash's scaler hints in one fixed mode. So the rasteriser hints each outline itself with skrifa, the outline reader under swash. It fills each outline with zeno, swash's own rasteriser, as swash's scaler does.

**Placement.** A glyph is placed by its font (ADR-1302 D2):

- Its advance is rounded to the nearest pixel, and a negative advance counts as none.
- Its pen starts at its cell's left edge. The pen moves right by half of what its cells' width exceeds that advance, rounded down. When the advance is wider than the cells, that is a move left.
- Its ink sits on the baseline, at its bearing from the pen.

A narrow glyph drawn in two cells, such as a symbol widened by VS16, is centred across them by the same rule.

**Joining scripts.** An Arabic letter draws in the form that the console asks for. In a build with the bidi feature, the console keys each letter of a joining script with U+200D on each side that it joins. It asks for lam followed by alef as their ligature. It takes the joining sides from what vtium-term gives each cell (ADR-1004 D6). A letter asked for alone, as its codepoint or with its marks, draws its unjoined form.

A letter of a joining script is any character to which `ArabicShaping.txt` gives the Joining_Type D, R, L or C (ADR-1302 D2). Those run from Arabic and its tatweel to Syriac, N'Ko, Mongolian, Mandaic and Adlam. Such a letter is stretched sideways before it is rasterised, with its pen on its cell's left edge. The stretch is its cells' width over its advance as the font gives it, unrounded. So its advance fills its cells, and its joining strokes meet the next letter's strokes with no seam.

A shaped cluster of one joining letter is stretched as one. That includes the marks that HarfRust placed on the letter, and their offsets, so each mark stays over its letter. The advance is what is stretched. So ink that a font designed past its advance stays past it, and reaches the neighbour by ADR-1302 D1.

A cluster of more than one joining letter is not stretched, and keeps its pen on the cell's left edge. The lam-alef ligature that the console asks for across lam's and alef's two cells is such a cluster. It is judged as a joining form whether or not it holds U+200D (`shaping::kind`).

A joining form is a letter keyed with U+200D on the sides it joins. The font draws its letter with one glyph or with several, as Noto Naskh Arabic draws beh as a body and a dot. The joining form is kept when those glyphs all come from one character or one ligature, and differ from what the cluster draws with nothing joined.

**Marks the font lacks.** Sometimes the font cannot draw a letter with marks, or a joining form, whole, because it has no glyph for some of the marks. For example, DejaVu Sans Mono has no glyph for the superscript alef. Then the cluster is shaped again without those marks. The part that the font draws is sent under its own key. So the console draws that part as the font shapes it, and lays the missing marks over it (ADR-1200 D22 step 3, `shaping::shape::shape_all`).

Only a console built with right-to-left text takes parts. It asks for them with the request kind `ASK_PARTS`. A plain `ASK`, which the minspec build sends, gets no parts. So the minspec console carries no code for them. A part goes once in an answer, and never under the key of a cluster that the console asked for (`tests/serve.rs`).

**Ink past the cells.** Nothing moves a glyph to keep its ink inside its cells. The glyph's pixels are cut at the cells' edges. A glyph whose ink passes its cells also carries its ink box (ADR-1302 D1, D6, ADR-1306 D1, D3). The box keeps the glyph's ink up to its reach, and cuts it there. The reach is one cell past the glyph's cells on each side, and one row above and below its row (vtium-font's `REACH_CELLS` and `REACH_ROWS`). The box covers the rows and columns that span the glyph's cells and its ink. The box is sent in the set's `INKX`, beside the glyph's cut pixels.

So DejaVu Sans Mono's accented capitals keep their whole accents in a 16-pixel cell. There the font's ascent and descent fill the cell, and the accents pass the cell's top by up to 2 pixels (`tests/accents.rs`). The console draws the accents into the row above. A glyph whose box would be larger than a set's 16 KiB chunk carries no box, and stays cut to its cells. Color glyphs fit their cells and carry no box.

A shaped cluster's marks are fitted to the glyph's reach: a row above and a row below its own row (ADR-1302 D3, the maintainer's decisions of 2026-11-01). ADR-1306 D2 holds the maintainer's decision of 2026-11-06, which moved the rule's edge from the row to the reach. Inside the reach, a mark draws where HarfRust places it. So Arabic marks and Vietnamese stacked accents draw where the font puts them. `tests/accents.rs` checks E, A and O with their marks, in a DejaVu Sans Mono subset that lacks the precomposed capitals.

Where the font holds the precomposed letter, HarfRust composes the sequence into that letter's glyph, so the two draw alike. Where HarfRust attaches the mark instead, the accent lies where the font's anchor puts it. That place can differ from the precomposed glyph by a pixel or two, as É's acute does at 32 pixels in DejaVu Sans Mono.

A glyph counts as a mark by the character that it was shaped from. vtium-config gives HarfRust each character of the cluster, numbered by its place in the cluster. HarfRust tags each glyph that it returns with the number of the character it came from. So vtium-config looks that character up. The glyph is a mark when its character has the General_Category Mn or Me, or a combining class other than 0 (`MARKS` in `src/shaping/data.rs`). HarfRust returns a right-to-left cluster in visual order, with the marks before their letter. The lookup by character still finds the marks there.

Some marks lie above or below their base, but the shaper leaves them at the pen, with no advance and no offset. For example, HarfRust leaves an acute after a circumflex that way in DejaVu Sans Mono. The helper first places such a mark itself. It centres the mark over its base's ink, either a row above all the ink since the base, or directly under the base. The mark's base is the letter before it in the order that the text is written. Unicode's combining character sequence and OpenType's mark-to-base attachment use the same order. In a right-to-left cluster, the base is found on either side of the mark. A letter that the font draws as several glyphs, such as a body and its dot, is one base.

The marks over the middle of the base letter's ink are one stack, and the marks under it are another. A stack whose ink passes the reach's top or bottom edge moves toward the baseline until it fits. It moves only while two tests hold. The move must land no more of the stack's ink on the letter's ink than lies there already. The move must also leave the stack inside the reach's other edge. When the move fails either test, the whole cluster, letter and marks together, is scaled vertically, just enough to fit the reach. The scaling keeps the font's arrangement. Each row of the result is the average of the rows that it covers.

The letter's own ink need not reach the reach's edge for that to happen. A request that gives a reach of no rows shows the rule on lam with shadda and fatha, in DejaVu Sans Mono at 16 pixels (`tests/shaping.rs`). That cluster has blank rows above the lam, but it is scaled, because its marks, moved down, would land on the lam's stem. No mark measured in DejaVu Sans Mono or Noto Sans Arabic passes a whole row. So the rule moves only a stack such as Tibetan's subjoined letters. Otherwise, base letters never move. A mark keeps the sideways place that the shaper gave it, and its ink reaches sideways as any ink does. A color cluster's glyphs are left as they are.

The helper finds each outline source's baseline at a cell's height once, from the source's whole printable ASCII set. It keeps that baseline for the engine's life. It hands the baseline to every request that it rasterises for that source and height. So glyphs sent for different requests, or sent again after an eviction, stand on one baseline, whatever each request held. Each font in a strike stands on its own baseline in its glyphs' pixels. A font with no printable ASCII keeps its ascent, rounded up.

The strike reports the baseline of the frit's first source, the font that sets the cell. It does so even after that source fails as it draws, so the answers at one cell never change mid-session. The strike's underline and strike-through follow that baseline:

- The underline's thickness and centre come from the font's `post` table. Without that table, the underline is a fourteenth of the cell, near its bottom.
- The strike-through's top and thickness come from the `OS/2` table's strikeout, when its size is above 0. Otherwise the strike-through is as thick as the underline. It is centred on half the x-height, which is `OS/2`'s `sxHeight` or the `x` glyph's height, or else on half the baseline.

Each line is at least a row thick, at most a quarter of the cell, and inside the cell. A strike whose first source is a bitmap font reports the cell's bottom row, and the lines of a face with no metrics of its own, as before.

The helper draws a glyph before the terminal asks for it, and no request names a width. So the helper gives each character or cluster the cells that the terminal gives it, by vtium-term's own rule (`vtium_term::cluster_width`, ADR-1200 D21). The rule gives two cells for East_Asian_Width W or F, for Emoji_Presentation=Yes, and for an emoji-style character with U+FE0F. It gives one cell otherwise. The glyph's advance never decides the width. A fallback font sized to the cell's height, as Noto Sans CJK behind DejaVu Sans Mono is, draws its ideographs narrower than one and a half cells.

A glyph narrower or wider than its cells has its advance centred across them. A wide icon of a Nerd Font that is not a Mono build takes the one cell that the terminal gives it. Its ink past the cell reaches by its ink box. A color glyph is a COLR glyph's layers, or a CBDT or sbix bitmap. A color glyph is drawn unhinted, centred across its cells, and kept inside the cell's height. COLR layers are laid over each other with the renderer's rounding, premultiplied. A COLR glyph larger than its cells is drawn again at the size that fits them.

A color bitmap is taken from the font's largest strike that holds it, with its colors premultiplied by its alpha. vtium-config scales it itself to the cell's size, or as far under as fits its cells. The method is Lanczos, then a light sharpen: an unsharp mask of radius 0.8 at 120 per cent. That is the operator's choice on vtium/vtium#23, from a comparison of six ways (`vtium-review-images/23/emoji-scaling.png`).

Four other methods stay in `src/resample.rs`: box, plain Lanczos, box with alpha snapped at half, and nearest. The tool is built with one method, chosen with `VTIUM_CONFIG_COLOR_SCALE=lanczos-sharpen|lanczos|box|box-snap|nearest`, where the first is the default. It is a build-time setting rather than Cargo features, because the methods exclude each other and features add up across a workspace. An unknown value fails the build. Weights, sizes and placement are computed in f64, with IEEE basic operations alone, and rounded to 14-bit integers. Pixels are integers. No `sin`, `exp` or `powf` comes from the platform. So a glyph is the same bytes on every host.

The parser's answer is checked by the parent. For each cell asked for, the answer gives the cell, the baseline, every glyph's width, format, pixels and ink box, and the map. The parent checks these things:

- every codepoint is one that the request covers;
- every RGBA pixel is premultiplied;
- no box is larger than a set's chunk;
- every box is an R8 glyph's, past its cells and inside its reach, which is the request's reach above and below the cell.

## Sources

- **kbd names** resolve as `setfont` resolves them (kbd's `src/libkbdfile/kbdfile.c`, `src/libkfont/context.c`). The helper looks in `consolefonts/` of each data directory in turn: `/usr/share/kbd`, `/usr/share`, `/usr/lib/kbd`, or `--kbd-dir`. It takes the name followed by the first of `""`, `.psfu`, `.psf`, `.cp` and `.fnt` for which a file exists, plain before `.gz`, `.bz2`, `.xz` and `.zst`. A name with a `/` looks in the subdirectory that its first component names. An absolute path is taken as given, then with each suffix. Unimaps resolve the same way under `unimaps/`, with `""`, `.uni` and `.sfm`.

  A font or unimap found by name is resolved through its links. It is read only if it stays under the `consolefonts/` or `unimaps/` of a data directory. A link out of them fails the source, and nothing is read from outside. The helper never looks for a relative name in the directory it runs from, as `setfont` does first. The helper runs unattended, and a file there would shadow the data directories. A `font:` family's scan fails past `FAMILY_ENTRIES_MAX` entries in one directory. An outline font is opened to read its family only if it stays under the font directories once its links are resolved. A font named for the family that leaves them through a link fails the source.
- **A font's map** is the frit's `unimap=` when given, else the font's own table, else kbd's `def.uni`. A font with none of these is refused, naming `unimap=`. A named font takes `def.uni` too, because the helper has no kernel map to inherit. `setfont`, by contrast, loads `def.uni` only for its default font, and otherwise leaves the kernel's map in place. A unimap entry for a glyph past the font's last glyph maps nothing.
- **vconsole** reads `FONT=` and `FONT_UNIMAP=` from vconsole.conf. The kernel command line's `vconsole.font=` and `vconsole.font_unimap=` go over them, as systemd-vconsole-setup reads them (`src/vconsole/vconsole-setup.c`). With no `FONT=`, systemd ran no `setfont`, and the kernel kept its font. So the source is refused, and the refusal names `console:VT`. `FONT_MAP=` maps the kernel's 8-bit translation and sets no glyph, so it is not read.
- **console:VT** takes the font that the console read from its own VT at start, and handed to the helper (ADR-1200 D8). The console reads the font with `KD_FONT_OP_GET_TALL`, or with `KD_FONT_OP_GET` on a kernel without it, and reads its map with `GIO_UNIMAP`. The helper opens no VT.
- **CPI fonts** are the codepage files that kbd ships (`.cp`), read as `setfont` reads them. A file holds one screen codepage, with its 28-byte entry header and its info header. Then come each font's height, width, aspect and glyph count, and its glyphs. A CPI file holds a font of each of several heights. So its source takes `cp-size=` 8, 14 or 16, as `setfont` takes `-8`, `-14` or `-16`. With no `cp-size=`, the source fails, naming `cp-size=`. `cp-size=` on a source that reads as no CPI file fails that source. A CPI font's glyphs have no table, so they take `unimap=`, else `def.uni`.
- **BDF fonts** are read by vtium-config's own reader, never through swash (ADR-1200 D14). The reader takes Adobe's BDF 2.1, plain or compressed, in the character set ISO10646-1, whose encodings are codepoints. Any other character set fails the source. The registry and the encoding are the `CHARSET_REGISTRY` and `CHARSET_ENCODING` properties. Where a property is missing, they are the last two fields of the XLFD name on the `FONT` line. So Tiny5 Mono's BDF, which names `iso10646-1` only there, reads as published. A font that gives the registry `ISO10646` and no encoding at all reads too.

  The font's cell is as wide as the advance that most of its glyphs take, the narrower on a tie. So one stray glyph narrower than the rest leaves only itself out. The cell is `FONT_ASCENT` and `FONT_DESCENT` high, else the bounding box's height. A glyph advancing that width is one cell wide, and a glyph advancing twice it is two. A glyph of another advance is left out, and so is a glyph whose encoding is no Unicode scalar value. A codepoint's first glyph is taken.

  A glyph of `ENCODING -1` has no place in the character set. It is read as any glyph is, so a malformed one still refuses the font. It is then left out, and not counted against `CHARS`. Some fonts, Tiny5 Mono's among them, count `CHARS` without such glyphs. Each glyph sits on the baseline at its `BBX` offsets, and is cut at its cells' edges.

  A BDF font's bytes are read where the source is resolved. They are parsed in `Engine::load`, in the helper's own sandbox with the other fonts, since the glyph form that the parser's child answers with holds no glyph two cells wide. `preview` (#120) has no sandboxed place to parse one yet.
- **Compression** is told by its magic, as kbd tells it. It is decoded with its stream's own check value verified. The formats are gzip, bzip2, xz and zstd. Each is refused past `COMPRESSED_BYTES_MAX` before it is decoded, and past `PSF_BYTES_MAX` once decoded.

## Decoders

Each decoder is pure Rust, compiles no C and has no build script, and only this binary links it. `cargo tree -p vtium -e normal` lists none of them, so they add nothing to the minspec binary. The operator approved them on 2026-10-28.

| Format | Crate | Licence | Why this one |
|---|---|---|---|
| gzip | `miniz_oxide` 0.8.9 | MIT, Zlib or Apache-2.0 | Already in every vtium build for the built-in sets' deflated chunks. vtium-config frames gzip around it. |
| bzip2 | `bzip2` 0.6.1, on `libbz2-rs-sys` 0.2.5 | MIT or Apache-2.0; `libbz2-rs-sys` under libbzip2's BSD-style licence | Trifecta Tech Foundation's Rust port of libbzip2. It is the default backend of the most used bzip2 crate, maintained (0.2.5, May 2026), with `build = false`. Its one C file is a header that no build compiles. `bzip2-rs`, decoder only, has had no release since 2021. |
| xz | `lzma-rust2` 0.24.0, features `std` and `xz` | Apache-2.0 | Maintained (October 2026), and verifies the stream's check. With no default features, it adds only `sha2` and its RustCrypto crates (MIT or Apache-2.0). `lzma-rs` has had no release since January 2023. `xz2` and `liblzma` compile liblzma's C. |
| zstd | `ruzstd` 0.9.1, default features | MIT | The pure-Rust zstd decoder (October 2026), with `build = false`. `hash` adds `twox-hash` (MIT) to compute each frame's checksum. vtium-config compares that checksum with the frame's own, as `ruzstd` leaves that to its caller. 0.9.1 is the first release that bounds each block's decoded size (RUSTSEC-2026-0344). 0.8.2 let a 64 KB file ask for gigabytes in one block. It needs Rust 1.87, inside the workspace's `rust-version`, 1.88. `zstd` binds the C library. |
| TrueType, OpenType, COLR, CBDT | `swash` 0.2.10, default features | MIT or Apache-2.0; under it `skrifa` 0.44, `read-fonts` 0.41 and `font-types` 0.12 (MIT or Apache-2.0, Rust 1.85), `zeno` 0.3 and `yazi` 0.2 (Apache-2.0 or MIT), and `bytemuck` 1.25 (Zlib, MIT or Apache-2.0) | ADR-1200 D14's decided rasteriser. It is the only pure-Rust one with hinting, embedded bitmaps and color, with `build = false`. ab_glyph has no hinting, bitmaps or color. fontdue holds every glyph at load. |
| Outline hinting | `skrifa` 0.44.0, its `std` feature alone | MIT or Apache-2.0; the version and features swash already builds, so it adds no code to the build | swash's scaler hints in one fixed mode, and `hinting=` (ADR-1200 D1) needs light and full. The rasteriser calls skrifa's hinters itself. |

The tests' xz fixtures take `lzma-rust2`'s encoder as a dev-dependency. The bzip2 and zstd fixtures use the encoders that the decoding crates carry.

## The parser

`src/child.rs` is the side of `vtium-config --parse` that reads the request. The parent's side, which starts the parser, lives in `tests/common/parser.rs`. The child runs for one font. It has an empty environment, `/` as its directory, a process group of its own, its three pipes, and no other descriptor. Run as root, the parent starts the child as `nobody` from `/etc/passwd`, with its supplementary groups dropped. The parent refuses to start it when there is no such account.

The parent kills the parser through a pidfd past 60 s by the wall clock. It reads the parser's stderr on a thread of its own. Another thread reaps the parser as soon as it exits, even while a process that it started still holds its pipes. Just before the reap, that thread kills the parser's process group, so nothing that the parser started outlives it. The parser itself may start no process. So the group catches what a stand-in starts in the tests, and anything that slips past that limit (vtium/vtium#163).

The parser sets no-new-privileges, an address space of 512 MiB, 30 s of CPU, no file writes, no processes and no core. It refuses to run as root. It reads its request, parses, and answers with the glyph form. The parent checks that answer field by field against the PSF bounds before it trusts it. A parser that fails, faults, passes a limit or answers malformed fails that source.

No command runs the parser since `create` went, and only the tests start it. `preview`, `fonts`, `new` and the TUI hand fonts to the helper, `serve`. The helper sets the same limits on itself and refuses root. So no command needs the parser's child (ADR-1201 D3, D9). The helper takes its `PARSER_MEMORY`.

## The helper

`vtium-config serve` is the helper that the console starts on its first need for a frit's glyphs (ADR-1301 D2).

**Start.** The helper first sheds every descriptor above standard error that its starter left open. Such a descriptor may be a copy of a terminal that a font could write to. It marks those descriptors close-on-exec, and starts itself again once through `/proc/self/exe`. It sets `VTIUM_CONFIG_SERVE_SHED` so that it does this only once. After that it holds only its three pipes. It then takes these steps, in order:

1. It sets no-new-privileges.
2. It installs its socket filter.
3. It enters its landlock domain. On a kernel without landlock, it installs its fallback filter instead.
4. It starts the thread that reads its requests.
5. It sets the parser's limits, with 300 s of CPU over its life.
6. It reads its ids from `/proc/self/status`.

**The landlock domain and the socket filter.** The maintainer decided on these on 2026-11-04 (ADR-1200 D15). Both come before any font is read, and before the reader's thread starts, so that thread inherits both. The fallback filter, which stands in for the domain on a kernel without landlock, comes at the same point. The landlock domain uses the Linux landlock maintainers' `landlock` crate 0.4.7, the version that the image helper builds. It refuses every right to write, create, remove, rename or truncate a file. Reads stay open, since a frit may name a font by an absolute path anywhere. The pipes that the helper already holds are not affected.

What the domain refuses on the network depends on the kernel's landlock version:

- from version 4 (Linux 6.7), binding and connecting TCP sockets;
- from version 6, reaching an abstract Unix socket made outside the domain, and signalling a process outside it;
- from version 9, connecting to a named Unix socket.

The domain never refuses a UDP socket. The socket filter closes what landlock leaves open. It is a seccomp filter. Under the filter, `socket`, `socketpair`, `io_uring_setup`, `io_uring_enter` and `io_uring_register` answer EPERM, and every other call goes on. So the helper makes no new socket of any kind on any kernel. io_uring is refused whole, because its socket operation, from Linux 5.19, makes a socket without calling `socket`. On 32-bit ARM, `socketcall` (call 102) answers EPERM too. A kernel built with `OABI_COMPAT` still takes the old ARM ABI's calls from an EABI program, and the old ABI makes every socket through `socketcall`.

**The fallback filter.** The maintainer decided on this filter on 2026-11-13 (ADR-1200 D15). Landlock is missing on kernels before Linux 5.13, and on kernels that are built without it or that leave it out of their active security modules. On such a kernel, the helper installs a second seccomp filter in place of the domain. The filter refuses the calls that the domain refuses, as far as a filter can tell them apart by their numbers and their arguments. Each refused call answers EPERM:

- `open` and `openat` with a write access mode, `O_CREAT` or `O_TRUNC` (`O_TMPFILE` needs a write access mode), and `creat`;
- `mkdir`, `mkdirat`, `mknod`, `mknodat`, `rmdir`, `unlink`, `unlinkat`, `rename`, `renameat`, `renameat2`, `link`, `linkat`, `symlink` and `symlinkat`, which make, remove, rename and link files, folders, device nodes and FIFOs;
- `truncate`, and `truncate64` on 32-bit ARM, which truncate a file by its path;
- `kill`, `tgkill`, `rt_sigqueueinfo` and `rt_tgsigqueueinfo` to any process but the helper itself, and `tkill` and `pidfd_send_signal` always;
- `fcntl` with `F_SETOWN` or `F_SETOWN_EX`, which point a descriptor's signals at another process, and `fcntl64` with them on 32-bit ARM;
- `ptrace`, `process_vm_readv`, `process_vm_writev`, `process_madvise`, `pidfd_getfd` and `kcmp`, which the kernel checks as it checks tracing another process;
- `ioctl`, except `FIOCLEX`, `FIONCLEX`, `FIONBIO` and `FIOASYNC`, the four commands that act on the descriptor itself and that the domain allows on a device too.

`openat2` keeps its flags in memory, which a filter cannot read. So under the fallback filter `openat2` answers ENOSYS, as on a kernel that lacks it, and a program falls back to `openat`. A program that changes a file's mode, owner, times or extended attributes passes both the domain and the filter, since landlock does not handle those rights. A read of another process's memory through `/proc/<pid>/mem` is an ordinary open for reading, so the filter cannot refuse it. The kernel's own checks on tracing decide it. Where the kernel offers an older landlock version, the helper enters the domain and installs no fallback filter, so the domain refuses only what that version knows.

**How the filters are built.** The helper builds both filters itself, in `src/seccomp.rs`, as BPF programs (the kernel's filter language). It keeps a table of call numbers for each architecture vtium builds for: x86_64, aarch64, riscv64 and 32-bit ARM. `seccompiler` 0.5.0, the version that the image helper builds, only installs them, since it builds no program for 32-bit ARM. So the armv7 helper runs under both filters, as the others do. Each program first checks the call's architecture, and ends the helper when a call comes under another architecture's numbering, since it cannot read that call's number. On x86_64, each program then refuses every call of the x32 numbering, whose numbers would match no rule. On an architecture with no table, the helper runs without either filter.

Any failure to install a filter or to enter the domain sends a failed frame.

**The sandbox's report.** The helper tells the console which parts of its sandbox are in force. It does so in a frame of its own, kind 14, which leads its answer to the first request. The frame holds one u32 of bits: 1 for the landlock domain, 2 for the fallback filter and 4 for the socket filter. The helper sends the frame only once a request has shown that the console comes from the same build, so a console of another build is told that instead. When the domain is missing, the console logs one line, once a boot. The line says whether the fallback filter stands in for the domain. When the socket filter is missing, the line says so too (`crates/vtium/src/helper.rs`, `sandbox_notice`). vtium-config's own commands, which start the helper too, pass over the frame.

**Root.** The helper refuses to run as root, when any of its real, effective, saved and filesystem uids is 0. It refuses to run with root's group, when any of those four gids is 0, or when its supplementary groups include 0. A saved id of 0 would let it take root's id back. It sends a failed frame naming why, and the console gives the helper up for the boot. The helper does not refuse supplementary groups in general. Run by a user, as `preview` and `fonts` run it, it holds that user's groups, which give it nothing that its starter lacks.

The console checks the drop that it makes itself. When the console runs as root, it starts the helper as `nobody`. std drops the supplementary groups before it sets the uid, and goes on when the kernel refuses it that. So the console reads the helper's ids back from `/proc/<pid>/status` as soon as the helper runs, before it sends a request. It refuses a helper whose four uids or four gids are not `nobody`'s, or that holds any supplementary group (`crates/vtium/src/helper.rs`, `Launcher::dropped`). So the console never sends a request to a helper whose drop to `nobody` did not take.

**Drawing.** Each request carries the frit's name and text, so a frit that the console makes on the fly needs no file. The helper reads the frit's fonts itself, as the account that the console started it as. It parses bitmap fonts and rasterises outline ones in its own process. It draws at the cell that each request names, the frit's cell, never another (#145). It draws an outline font at that cell. It scales a bitmap font by the largest whole factor that fits, and places it on the cell's bottom row. A frit's `console:` source takes the kernel's font that the console read at start and handed over.

**The cell query.** The helper takes four requests. The first is a cell query, which the helper answers at once (ADR-1200 D17). The console sends it before its first frame, for a frit that states no `cell=`. For an outline frit, the console also sends it at each zoom.

The frit's first source sets the cell. That is the first source in the frit's order that the helper can draw from. As for glyphs, the helper leaves out a source that this machine does not have or cannot read. It also leaves out a source that has failed as it drew, including a source whose W rule fails. The next source then sets the cell instead. ADR-1200 D17 says only that the first source sets the cell. Which source counts as first is the helper's own behaviour.

A bitmap first source answers its own cell, once every later bitmap source's cell fits in it. The console takes that cell at whole multiples. An outline first source answers the cell that the W rule gives it, at the height that the query names. The console takes that cell as it is. A query that names no height gets no cell for an outline first source. Later sources of either kind draw into that cell. The helper reads no `cell=` line to answer a query, since the console reads `cell=` itself, and asks only where D17 has it ask. The console takes either answer to any query, its own cell (frame 6) or the W rule's (frame 10), whatever the frit's text names first. So when an outline first source is missing on this machine, a bitmap source after it answers, and the reverse.

**Ask, fill and stop.** The other three requests are these:

- An **ask** is answered whole, ahead of anything else, with the glyphs of the codepoints and the clusters that it names. Each cluster is shaped with HarfRust from the first outline source in the frit's order that covers its first scalar value other than U+200D. The cluster is drawn whole into the frame's cluster map (ADR-1200 D21).
- A **fill** draws a cell's coverage while nothing is asked. It draws the codepoints that it names first, then printable Latin and box drawing, then the rest, 1,024 codepoints a batch. It skips what the console holds and what the helper sent. It takes a waiting request before each batch.
- A **stop** ends a cell's fill.

Some codepoints and clusters ask for emoji presentation under UTS #51. One does so when it holds U+FE0F. One also does so when it holds no U+FE0E and either starts with a character of Emoji_Presentation=Yes or is an emoji sequence such as a flag. Such a codepoint or cluster is drawn from the first source, in that order, that gives it a color glyph. The color glyph of a character alone counts for the character and U+FE0F. Only when no source gives a color glyph is it drawn from the first source that gives it any glyph (`PRESENTATION` in `src/shaping/data.rs`). A cluster that no source shapes as its kind requires is left out, and the console draws it its own way.

**Styles and scripts in a request.** An ask names a style: regular, bold, italic or bold italic (ADR-1200 D21). A fill, a stop and the queries are regular. So the fill fills only the regular glyphs, and the other styles come by ask. A request of another kind that names a style is malformed. An ask also names a script: normal, superscript or subscript (ADR-1202 D11). A request of another kind that names a script other than normal is malformed.

The helper draws a script glyph from the source that draws the character's normal glyph, and from no other (ADR-1202 D15):

- It rasterises an outline source's glyph at two thirds of the size at which it draws the normal glyph. It stands the glyph on the script baseline. The script box is the first source's own box, its rows from ascent to descent or a bitmap font's cell as drawn, made two thirds as tall. The box is set against the text band's top for a superscript, and against its bottom for a subscript. The baseline lies the same share of the way down the box (ADR-1202 D1, D2).
- It scales a color glyph once from its source, fitted to two thirds of its cells. It places the glyph against the cell's top or bottom (ADR-1202 D5).
- It draws a bitmap source's glyph upright and unstyled. It makes the glyph small with vtium-font's script rule, `Glyph::scripted`, which the console's own bitmap fonts take too, with each pixel fully on or off. It then gives the glyph its bold and italic (ADR-1202 D4, D7).

The outline baseline and the color glyph's box come from the same module's `script::size` and `script::place`. A script glyph keeps no ink above or below its row (ADR-1202 D8). A superscript or subscript strike reports the lines of the small glyph's own size, where its own box lies. The console draws a script cell's underline, strike-through and overline on those lines (ADR-1202 D8). For an outline first source, the helper measures the font's `post` underline and `OS/2` strikeout at two thirds of its size, from the script baseline. For a bitmap first source, it takes the lines that vtium-font's `script::lines` gives. Those are the smaller size's own lines placed with its glyph, or the normal lines sent through D4's map.

**The request's layout.** The style, the script, and the rows that line spacing adds above and below the cell are fields of the request. The first answer starts with the sandbox's report (kind 14), which an older console cannot read. So the request's magic is `VTVSREQ8`, which also marks the coverage query and a color emoji's ink box. A request of an earlier layout, from `VTVSREQ7` back to `VTVSREQ3`, is refused whole. The refusal says that the console and the font helper come from different builds, and that reinstalling vtium mends it. The console says the same when an older helper refuses its request. Each glyph frame, each fill's end, and each stop taken names the band as well. So the console holds glyphs by the cell and its band (Line spacing, below).

**vtium-config's own requests.** A fifth request, a sources query, is vtium-config's own, and the console never sends it. The helper answers it at once with each source of the frit that it left out. It marks each one `missing` when this machine does not have it, and `refused` when the machine has it and cannot read it. It gives the source's line, spelling and why, which `fonts` lists.

A sixth request, an ask that says how its glyphs were made (kind 7), is vtium-config's own as well. The helper answers it as it answers an ask. Before the done frame, it sends a frame of its own, kind 13. That frame says how the helper made each superscript and subscript glyph that it drew. The ways are an outline rasterized small, a color glyph scaled smoothly, the font at a smaller whole multiple, or another CPI height. They are also a `script-source=` font by its spelling, or the normal glyph scaled with each pixel on or off. The font preview and the TUI's font view name that way in the caption of their line of superscripts and subscripts (ADR-1202 D14).

**The coverage query.** `vtium check-config` sends a coverage query (kind 8) for each look's font set. The query names the letters of the system locale's script that the built-in fonts would squeeze (ADR-1401 D7, ADR-1302 D14). The helper answers at once, with frame kind 15. The frame lists the letters that the font set's sources cover. It then names the family of an installed outline font that covers the most of the other letters, or nothing when no font does. The helper looks at no more than `FAMILY_FILES_MAX` font files for that. `vtium check-config` prints a note, outside its exit status, when the font set leaves any of those letters to the squeeze.

**A color emoji in one cell.** A color glyph that must draw in one cell, such as ❤ (U+2764) with no variation selector, is drawn as large as a 48×96 cell draws it. vtium-font's `Glyph::in_one_cell` then shrinks the square around its ink to the cell's width and 2 pixels, by nearest neighbour (ADR-1302 D5). Its 1-pixel edge on each side travels as an RGBA ink box. For a character that presents as text by default, a later source's text glyph comes before an earlier source's color glyph in one cell (ADR-1302 D14).

For a frit that it draws nothing from, the helper answers with a frame of its own, kind 12. That happens because the frit does not read, or none of its sources is on this machine. The failed frame, kind 3, is the helper's own failure. The helper also sends it when it fails as it starts. So vtium-config never takes a failed frame as a frit's answer. Every frit still unanswered is unknown, with the helper's reason.

**Answers.** Glyphs go as vitrified sets of one cell, each strike in the style and the script that the request named. A styled ask's set holds only that style's strikes. A superscript or subscript ask's set holds only that script's strikes, which vtium-font's set format 2.3 allows. The helper sends each warning that a frit's text gives, such as `compress=`'s, once, as it first reads the frit. The warning names the frit and the line, and the console logs it. A source that fails as it draws is left out for the helper's life, and the next source in the frit's order gives its glyphs. A fill that never draws a glyph fails, naming each source whose cell does not fit, or that failed.

**Bounds.** Each request is bounded before anything is allocated for it:

- the frit's name at 64 bytes, and its text at `FRIT_BYTES_MAX`;
- the cell inside `CELL_SIDE_MAX`;
- the style one of four, and the script one of three;
- the display's capabilities reserved and empty;
- 32,000 codepoints, each a scalar value;
- 32,000 clusters, in an ask alone, each 1 to `CLUSTER_SCALARS_MAX` scalar values;
- 65,536 held ranges;
- the kernel's font inside the kernel's own bounds.

A frame's payload is at most 16 MiB, and a batch past that is cut in two. A glyph can be larger than a set's 16 KiB chunk. For example, each glyph of an 8×16 bitmap font drawn at 33 times its size is that large. Such a glyph goes in a chunk of its own, and is sent like any other (ADR-1302 D6). A batch of such glyphs must never outgrow the helper's memory limit. So the helper draws at most as many glyphs at once as a frame holds at their largest size. For a frit with an outline source, that size is two cells of RGBA, since the helper may draw in color. For any other frit, it is two cells of 1-bit rows. A single glyph larger than a frame is left out, and the console draws that character in its built-in font.

The helper exits after `HELPER_IDLE_MS`, 60,000 ms, with no request and no fill. It also exits when the console closes its end and the helper's work is done.

## Styles

A frit gives each source a style with `style=` (ADR-1200 D1). The helper draws a style's glyphs from the sources of that style. It synthesises a style that the frit gives no source for (ADR-1200 D21, ADR-1302 D4). `serve::steps` lists what each style is drawn from, in order. Each step draws only what the steps before it left:

| Asked | Drawn from, in order |
|---|---|
| regular | the regular sources |
| bold | the bold sources; then the regular sources emboldened |
| italic | the italic sources; then the regular sources slanted |
| bold italic | the bold-italic sources; then the regular sources emboldened and slanted |

The frit may name a bold or an italic face but no bold-italic face. Then the helper draws bold italic from the regular glyph, emboldened and then slanted, and not from the bold or the italic face (the maintainer, 2026-11-04). The fill draws regular glyphs alone. Bold and italic come only when the console asks for them (the maintainer, 2026-11-04).

- **Synthetic italic of an outline glyph** slants the outline before it is rasterised. Each point moves right by 0.2 of its height above the baseline. So ink above the baseline leans right, and descenders lean left. The glyph leans right in every script.
- **Synthetic bold of an outline glyph** grows each contour sideways by a 48th of the size in pixels to the em on each side, a 24th in all. FreeType's `FT_GlyphSlot_Embolden` grew a glyph by the same amount. The growth is at least half a pixel a side, so a whole pixel in all at small sizes (the maintainer, 2026-11-04). The glyph grows only from side to side. Each point moves along the mitre of its two edges' outward normals, sideways only. A point where its edges fold back by more than 160 degrees stays put. No ink rises above the font's tallest ink, or falls below its lowest. So the row's baseline and lines hold for every style. ADR-1302 D4 leaves how a glyph is emboldened to the helper. Vertical growth was left out, because it would push ink across the row's edges, which D3 cuts.
- **A bitmap glyph** is widened for bold: a pixel is set when it or its left neighbour is set, inside the glyph's width. The glyph is then sheared for italic at its font's own size, before the whole-factor scale, by vtium-font's rule (`vtium_font::italic`). So the console's own bitmap faces and the helper draw the same glyph. A bitmap font is sheared about the baseline that it states, which is a built-in set's strike's baseline or a BDF font's ascent. Otherwise it is sheared about the row under the lowest ink of its `H`, and otherwise about its cell's bottom edge.
- **Bold italic** widens first, and then slants or shears the widened glyph (ADR-1302 D4).
- A slanted, emboldened or sheared glyph keeps its advance and its cells. The ink that it puts past its cells goes to its ink box (ADR-1302 D6). When that box's 1-bit rows would be larger than a set's chunk, 16 KiB, the glyph is sent with no box, cut to its cells. A color glyph is drawn as it is.
- The baseline, the underline and the strike-through of every style's strike are those of the frit's first regular source. So a styled glyph stands on the regular glyphs' baseline. A bitmap first source's strike states the baseline that it is sheared about, as its glyphs sit scaled on the cell's bottom row. So vtium-font's set reader, which shears a regular 1-bit strike drawn for italic, leans its glyphs as the helper does. The strike's underline and strike-through stay those of a face with no metrics of its own.
- The W rule's cell takes the frit's bold face too. It takes the first outline source given `style=bold`, and uses the wider of its `W` and the regular face's `W` (ADR-1200 D17). The frit's first regular source sets the cell.

## Line spacing

Each ask and fill names the rows that line spacing adds to its cell, above and below the text band (ADR-1302 D13). The helper draws each glyph in the text band, the cell less those rows, exactly as it draws one with no spacing. It then places the glyph in the whole cell by the `fit=` of the set that gives it, or by the character's default.

A `fill` glyph stretches to the whole cell. A bitmap glyph repeats its top row upward and its bottom row downward. An outline glyph's coverage is scaled vertically, with each new row the weighted average of the rows it covers.

A `natural` or `room` glyph keeps its size, with blank rows above and below. Its ink is not cut at the text band (ADR-1306 D8). The helper's outline request gives the rasteriser a reach of the rows that the band adds, and a row of the whole cell. So the ink above or below the text band lands in the added rows, which are part of the glyph's cell. Ink past those rows lands in the glyph's ink box.

A color glyph always keeps its size, whatever `fit=` says. A stretched glyph's ink box is cut to the text band, and stretched with the glyph. vtium-font's `spacing::place_rows` places each outline glyph and its box, as the console places its own. A box is left out where it would then be larger than a set's chunk. The strike's baseline, underline and strike-through move down by the rows added above. A cluster takes the fit of its first character other than U+200D.

## What this slice leaves for later

- The density is the first measured display's, since the TUI cannot tell which display shows the terminal it runs in. The TUI reads the EDID's size in whole centimetres, where the console reads the connector's millimetres. So the two can differ by a dot per inch or two. vtium-platform's EDID reader is private to it. Once vtium-platform makes it public, `src/display.rs` takes it in place of its own.
- A session's end returning its console to the configured font (ADR-1201 D11) is the console's work, in crates/vtium.
- vtium.conf's `font-hinting`, and a look's, over the frit's `hinting=` (ADR-1200 D1). The helper hints as the frit says, since no request carries the console's setting yet.
- ADR-1300's subpixel strikes, and shaping across cells for ligatures and syllabic scripts. The helper shapes each cluster that the console asks for on its own: the clusters that the terminal segments, and the joined forms of right-to-left text (#144).
- COLR version 1 glyphs, such as `Noto-COLRv1.ttf`'s, are not drawn in color, because swash reads version 0. A COLR layer in the text's color takes swash's mid gray, since a set cannot know the text's color.
- The parser's account is `nobody`. ADR-1400's pre-login account takes its place once vtium/vtium#31 names it.
- A root run is unverified on the maintainer's machine: the drop to `nobody`, `nobody` executing the binary from its install path, and the real `console:VT` ioctls. Tier 3's guest is where they run.

## Bounds

A frit and every source file are opened once, never blocking on a FIFO. Each is refused unless its descriptor is a regular file, and read no further than its bound.

| Bound | Value | Past it |
|---|---|---|
| `FRIT_BYTES_MAX` | 64 KiB | The frit is refused |
| `FRIT_LINE_MAX` | 4096 bytes | The frit is refused, naming the line |
| `FRIT_SOURCES_MAX` | 16 | The frit is refused, naming the line |
| `UNIMAP_ENTRIES_MAX` | 65535, kbd's count | The unimap is refused |
| A font file, a unimap | vtium-font's `PSF_BYTES_MAX` | The source is refused before it is parsed |
| `COMPRESSED_BYTES_MAX` | vtium-font's `GZIP_BYTES_MAX`, 1 MiB, in every compression | The source is refused before it is decoded |
| `ZSTD_WINDOW_MAX` | vtium-font's `PSF_BYTES_MAX`, 16 MiB, the window a zstd frame declares. `ruzstd` holds up to one declared window of decoded bytes, and a block, before it hands any out, so this bound is what limits a frame's buffer, beside `PSF_BYTES_MAX` on the output. `zstd font` and `zstd -19` through a pipe stay inside it; a font piped through `zstd --ultra -20` or above, or `zstd --long`, declares more | The source is refused as the frame's header is read, with a warning that names its window and says the font needs recompressing with a window of 16 MiB or less |
| A zstd block | the smaller of its frame's window and 128 KiB decoded, RFC 8878's `Block_Maximum_Size`, which `ruzstd` enforces | The source is refused as the block decodes |
| vconsole.conf, the kernel command line | 64 KiB | The source is refused |
| `GLYPHS_BYTES_MAX` | the largest PSF's glyphs and a map of every codepoint | The parser's answer is refused |
| `PARSER_MEMORY`, `PARSER_SECONDS` | 512 MiB, 30 s | The kernel ends the parser; the source fails |
| `PARSER_WALL_SECONDS` | 60 s | The parent kills the parser; the source fails |
| `FAMILY_ENTRIES_MAX` | 65536 | A `font:` scan of a directory past it fails the source |
| `FAMILY_FILES_MAX` | 4096 outline font files opened in one `font:` search | The search fails, naming the cap and the family |
| `FACES_MAX`, `NAME_BYTES_MAX` | 256 faces of a collection, 1 MiB of a `name` table | A face past them is not read for its family |
| `FONT_DEPTH_MAX` | 8 | A font directory's subdirectories deeper than it are not searched |
| `OUTLINE_BYTES_MAX` | 64 MiB | An outline font file is refused before it is parsed |
| `OUTLINE_ANSWER_BYTES_MAX` | 128 MiB, a strike of GB 2312's ideographs at 48 pixels and room | The parser's answer is refused |
| `FRIT_SIZE_MIN`, `FRIT_SIZE_MAX` | 6 and 96 points, one `size=` | The frit is refused, naming the line |
| `FRIT_CELL_MIN`, `FRIT_CELL_MAX` | 6 and 96 pixels a side | The frit is refused, naming the line |
| `OUTLINE_CELLS_MAX` | 3 cells an outline request names | The request is refused |
| `BDF_LINE_MAX` | 4096 bytes | The BDF font is refused, naming the line |
| `BDF_GLYPHS_MAX` | 65535 glyphs, as a PSF's: the `CHARS` line, and the glyphs it counts. A glyph of `ENCODING -1` is read and dropped, so only the decompressed file's size bounds how many there are, and none is held | The BDF font is refused |
| `BDF_BITMAP_MAX` | 192 pixels, a glyph's side or offset | The BDF font is refused, naming the line |
| A BDF font's cell | 1 to `FRIT_CELL_MAX` a side | The BDF font is refused |
| A BDF font's glyphs | `PSF_BYTES_MAX` in all, each counted as placed in its one or two cells | The BDF font is refused, naming the bound |
| A CPI file | 8 fonts of 512 glyphs, 32 wide | It is no CPI file |
| A glyph | a set's 16 KiB chunk | Left out: a color glyph two cells wide past about 64 pixels |
| `GLYPHS_MAX`, `MAP_MAX` | 65536 glyphs, a font's 16-bit glyph ids, and no more than the map names; 0x110000 codepoints | The parser's answer is refused |
| `RANGES_MAX` | 4096 `cover=` ranges a request names | The request is refused |
| `NAME_MAX`, `LICENSE_MAX` | 256 bytes of a family or format, 1024 of a licence | A request or answer is refused; a licence is cut there |
| A console font | 32 pixels wide, 64 high, 512 glyphs | The source is refused |
