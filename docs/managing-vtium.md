# Managing VTium

vtium is a terminal for the Linux virtual console. It runs on a VT, one of the full-screen text consoles the kernel provides, in the place of the getty, the program that prints the login prompt there. It takes the display through KMS/DRM, reads the keyboards and pointers itself, and draws text, color and pictures without a display server. This page holds the steps you take to run vtium day to day: installing it, giving it VTs, configuring it, choosing its font, and finding out what went wrong. [why-vtium.md](why-vtium.md) says why you might choose vtium, and [concepts.md](concepts.md) explains its pieces.

In the commands below, `#` marks a command you run as root, for example with sudo, and `$` a command you run as your own user. A line that starts `$ sudo` runs as root, but your own shell reads the line first, which matters where the line names your home directory.

## Installation

### Arch Linux

The AUR (the Arch User Repository) holds two recipes for vtium. A recipe is a `PKGBUILD`, the build script that makepkg reads to build a package. Install from one of them, since their packages conflict:

- `vtium` builds a release from the tag the maintainer signs, today the pre-release `v0.1.0-alpha.2`. It builds the packages `vtium` and `vtium-terminfo`.
- `vtium-git` builds the tip of vtium's `main` branch. It builds the packages `vtium-git` and `vtium-terminfo-git`, which provide `vtium` and `vtium-terminfo`.

The console package depends on the terminfo package, which installs `xterm-vtium`, the terminfo entry that sessions in vtium name in `TERM`.

makepkg refuses to build the `vtium` recipe until your keyring holds the maintainer's signing key, `33F9B54FD2A4B6A4110749B3BA464FC9847A832A`. The recipe carries a copy of the key, so import it once from there, then build and install:

```
$ git clone https://aur.archlinux.org/vtium.git
$ cd vtium
$ gpg --import keys/pgp/33F9B54FD2A4B6A4110749B3BA464FC9847A832A.asc
$ makepkg -si
```

`gpg --recv-keys 33F9B54FD2A4B6A4110749B3BA464FC9847A832A` fetches the same key from a keyserver instead. `vtium-git` checks no signature, because GitHub signs the merge commits on `main` with its own key.

The recipes build the minspec console, vtium's default and smallest build. `VTIUM_FEATURES` in makepkg's environment builds it with other features, for example `VTIUM_FEATURES='images logind' makepkg -si`. The `images` feature draws sixel and kitty graphics pictures, and the `logind` feature lets the suspend key ask logind, systemd's login manager, to suspend the machine.

The packages install these files:

| File | What it is |
|---|---|
| `/usr/bin/vtium` | the console, with its subcommands `vtium console`, `vtium check-config`, `vtium config`, `vtium blank` and `vtium wake` |
| `/usr/bin/vtium-config` | the font and settings tool, which `vtium config` runs: `vtium config tui` opens a tool that tries each font on the live console and saves your choice into vtium.conf, and `vtium config set` switches the font; it is also the glyph helper, which draws a font's glyphs for the console |
| `/usr/lib/systemd/system/vtium@.service` | the unit that runs vtium on one VT in place of its getty |
| `/usr/lib/systemd/system/vtium-getty@.service` | the fallback that starts a getty on the VT when vtium fails |
| `/etc/vtium/vtium.conf.example` | the example configuration, which lists every key at its default, commented out |
| `/usr/share/doc/vtium/examples/` | an example udev rule and hwdb fragment, which are documentation and never active |
| `/usr/share/terminfo/x/xterm-vtium` | the terminfo entry, from the terminfo package |
| `/usr/share/licenses/<package>/LICENSE` | the license, in each package |

Installing the packages gives vtium no VT and starts nothing. [Giving vtium a VT](#giving-vtium-a-vt) does that.

### From a checkout

Building vtium needs Rust 1.88 or later and clang. The checkout's `rust-toolchain.toml` names the toolchain the tests run on, and rustup installs it for you inside the checkout. The first build fetches a Debian sysroot to link against ([tools/arm/README.md](../tools/arm/README.md)).

On a machine with systemd and logind, `make vt-shell` builds the console, picks a free VT, asks for sudo, and runs your own shell in vtium there until you exit it. It installs nothing:

```
$ make vt-shell
```

To install a branch of your own as a package, point the `vtium-git` recipe at your checkout. makepkg builds the branch's last commit, so commit first:

```
$ VTIUM_GIT_URL=file:///home/me/src/vtium VTIUM_GIT_BRANCH=my-branch makepkg -si
```

On another distribution, `make install DESTDIR=DIR PREFIX=/usr` stages under DIR exactly the files the `vtium` package ships, and `make install-terminfo DESTDIR=DIR PREFIX=/usr` stages the terminfo entry, so that a package build can call them. Both refuse an empty DESTDIR and `/`, so neither installs onto the machine itself. The [vtium-arch README](https://github.com/vtium/vtium-arch) describes how the Arch recipes use them.

### Giving vtium a VT

`vtium console` gives a VT to vtium, or every VT, and takes them back. It changes which console systemd starts on a VT at boot, and which console logind starts when you switch to a VT where nothing runs. Each call of `enable` or `disable` takes one VT by its number, or `--all`. `enable` and `disable` need root, and `status` runs as any user.

See first what each VT runs now:

```
$ vtium console status
```

[Reading vtium console status](#reading-vtium-console-status) explains its columns.

To give vtium one VT, VT 3 here:

```
# vtium console enable 3
```

vtium is then VT 3's console from its next start: systemd starts `vtium@tty3` there at boot, in the place of the getty it disables. `enable` prints the command that starts vtium now. Log out of VT 3, run that command from another VT or over ssh, and press Ctrl+Alt+F3:

```
# systemctl start vtium@tty3.service
```

**Note:** `vtium console` starts and stops nothing, because starting or stopping a console on a VT ends every session there, the one you type in included. Each verb prints the `systemctl` command that makes its change now.

To give vtium every VT:

```
# vtium console enable --all
```

`enable --all` links `/etc/systemd/system/autovt@.service`, the template link that logind follows on every VT without a link of its own, to vtium. It enables vtium at boot on VT 1, where logind never starts a console itself, and it disables the gettys on VTs 2 to 6, so logind starts vtium on each the first time you switch to it. It leaves these VTs alone and names each one:

- a VT a display manager claims, as sddm claims VT 1, and a VT a graphical session or a greeter is on now;
- a VT where `kmscon` is enabled, linked or running, and a VT the development install holds ([The development VT](#the-development-vt));
- a VT whose own link, `/etc/systemd/system/autovt@ttyN.service`, names another console;
- VTs above 6, where logind starts no console by itself; `enable N` gives vtium such a VT at boot.

VT 6 is logind's reserved VT, where logind starts a console at every switch, even while the VT is in use. `enable 6` and `enable --all` refuse while someone is logged in on VT 6, unless that login is inside a vtium running there. A getty that waits at its login prompt on VT 6 holds no session, so both go ahead, and the next switch to VT 6 stops that getty. Where VT 6 has no link of its own, `enable --all` also refuses, rather than passing VT 6 by, while `kmscon` or the development install is enabled or running there.

On a machine where `kmscon` holds the template link, read [Moving from kmscon](#moving-from-kmscon) before you run `enable --all`.

Before `enable` changes anything, it records what each VT had in `/var/lib/vtium/console/`, and `disable` puts back exactly that:

```
# vtium console disable 3
# vtium console disable --all
```

`disable 3` takes vtium off VT 3's boot list, removes VT 3's own link to vtium, and gives VT 3 back the getty it had. After `enable --all`, though, logind still starts vtium on VT 3 through the template link whenever you switch to VT 3 while nothing runs there, and `disable 3` says so. Only `disable --all` points the template link back where it pointed before.

[crates/vtium/README.md](../crates/vtium/README.md#setting-it-up) gives the full rules. On a machine that was not booted with systemd, `vtium console` changes nothing and says so; there an inittab line, an init script or a supervisor runs `vtium --vt=ttyN --no-switchvt` where it ran a getty ([systemd.md](systemd.md)).

### Moving from kmscon

`vtium console enable --all` refuses while the template link names `kmscon`'s unit, `kmsconvt@.service`, and it passes by every VT where `kmscon` still runs. Disable and stop `kmscon` first, from a desktop terminal or over ssh, after logging out of each VT where it runs:

```
# systemctl disable kmsconvt@.service
$ systemctl list-units 'kmsconvt@*'
# systemctl stop kmsconvt@tty2.service
# vtium console enable --all
```

Run the `stop` line for each VT that `list-units` shows. Removing `kmscon` afterwards is up to you, with `pacman -R kmscon`; vtium needs nothing from it. vtium never reads `/etc/kmscon/kmscon.conf`. vtium reads the keys `kmscon` used, with the departures that [crates/vtium/README.md](../crates/vtium/README.md#departures) lists, so you can copy that file to `/etc/vtium/vtium.conf` by hand and check it with `vtium check-config`. [crates/vtium/README.md](../crates/vtium/README.md#sharing-vts-with-kmscon) says when to keep `kmscon-terminfo`.

### Upgrading and removing

An upgrade restarts no running vtium, since a restart ends every session on its VT. Each VT runs the new build from its next start. To restart VT 3 now, log out there, then from another VT or over ssh:

```
# systemctl restart vtium@tty3.service
```

Before you remove the package, give every VT back, and stop vtium where it runs ([the note above](#giving-vtium-a-vt) says why `disable` does not stop it):

```
# vtium console disable --all
$ systemctl list-units 'vtium@*'
# systemctl stop vtium@tty3.service
# pacman -R vtium
```

The records stay in `/var/lib/vtium/console/`, so after a reinstall `vtium console disable --all` still puts back what was there before. The [vtium-arch README](https://github.com/vtium/vtium-arch#removing-vtium) says what to do when you removed the package first.

## Configuration

vtium reads its configuration from `/etc/vtium/vtium.conf`. The package installs no such file, so vtium runs with its defaults until you write one. Start from the example:

```
# cp /etc/vtium/vtium.conf.example /etc/vtium/vtium.conf
```

The file holds one key a line. A key with a value is `key=value`. A switch is the key alone to turn it on, or the key with `no-` before it to turn it off. `#` starts a comment, except in a `#rrggbb` color. For example:

```
font-name=monoid
xkb-layout=de
dpms-timeout=300
bell
no-mouse
```

Each key is also an option on the command line, such as `--font-name=monoid` or `--no-mouse`. vtium reads `vtium.conf`, then the seat's file, `seat0.seat.conf`, in the same directory, then its command line, and each wins over the ones before it. `--configdir=DIR` names another directory than `/etc/vtium`, for a vtium you run by hand. [config.md](config.md) lists every key, its default and its bounds.

vtium reads its files once, when it starts: `vtium.conf`, the head file and the frits. A change reaches a VT at that VT's next start, so log out of the VT and restart its vtium, as in [Upgrading and removing](#upgrading-and-removing).

`vtium config` changes the font keys of vtium.conf and the profile each head of the head file draws with, and shows each changed line before it saves ([Choosing a font with vtium config](#choosing-a-font-with-vtium-config)).

The head file, `/etc/vtium/heads.conf`, decides which displays and keyboards make up each head, a terminal of its own for one or more displays ([heads.md](heads.md)). With no head file, vtium runs one head on every display. `vtium check-config` reads the configuration and the head file as a start would, applies nothing, and prints each problem it finds. Given a file, it checks that file in the head file's place, so you can check a new head file before you install it:

```
$ vtium check-config
$ vtium check-config ./heads.conf
```

It exits 1 when it finds a problem and 0 when it finds none.

## Fonts

vtium draws fonts from frits. A frit (a font set file) is a short text file, `/etc/vtium/fonts/NAME.vtfrit`, that lists a font's sources in order. The `font-name` key names the frit by its NAME, and the `font-frits` key names another directory than `/etc/vtium/fonts`. The console parses no font file itself. It starts the glyph helper, `vtium-config serve`, which reads the frit's sources and sends the console ready-made glyphs for its cell size.

With no `font-name` set, vtium draws the console font that `vconsole.font=` on the kernel command line or `FONT=` in `/etc/vconsole.conf` names, and with neither, its built-in fonts, Unifont and Unscii-8.

### An outline font

This frit draws Monoid Nerd Font Mono, with Noto Color Emoji for the emoji that Monoid lacks. Save it as `/etc/vtium/fonts/monoid.vtfrit`:

```
name=monoid
license=MIT
source=font:Monoid Nerd Font Mono
source=font:Noto Color Emoji
```

Then set it in `/etc/vtium/vtium.conf`:

```
font-name=monoid
```

The helper tries the sources in order, and each later source draws only the characters the earlier ones lack. The built-in Unifont and Unscii-8 draw every character that no source has. `name=` names the frit, and the file's name, `monoid`, is the default. `license=` records the font's license.

`source=font:FAMILY` finds a kbd console font of that name first. Without one, it finds an outline font file, `.ttf`, `.otf`, `.ttc` or `.otc`, whose name is the family, or the family followed by `Regular`, with case, spaces, `-` and `_` ignored. So `font:Monoid Nerd Font Mono` finds `MonoidNerdFontMono-Regular.ttf`. [crates/vtium-config/README.md](../crates/vtium-config/README.md#frits) lists every source form and frit key.

When vtium runs as root, as its unit runs it, it starts the helper as the user `nobody`. The helper looks for outline fonts only under `/usr/share/fonts` and `/usr/local/share/fonts`. A font installed in your home directory, for example under `~/.local/share/fonts`, never reaches it, so copy that font into `/usr/local/share/fonts`:

```
$ sudo install -Dm644 -t /usr/local/share/fonts/monoid-nerd-font-mono ~/.local/share/fonts/monoid-nerd-font-mono/*.ttf
# fc-cache -f
```

The first line runs through sudo from your own shell on purpose: your shell turns `~` and `*.ttf` into your home directory's files before sudo runs. In a root shell, `~` is root's home directory. The helper reads the font directories itself and uses no fontconfig cache, and `fc-cache -f` updates that cache for the other programs on the machine.

### A console font

A kbd console font from `/usr/share/kbd/consolefonts` needs no frit. A name that no frit bears is looked up as a kbd font, so this line draws `eurlatgr`:

```
font-name=eurlatgr
```

### Size

`font-size` in `vtium.conf` sets the size in points, 1/72 of an inch, which vtium converts to pixels at the display's density. An outline frit may set its own starting size with a `size=` line. A `font-size` above 0 wins over the frit's `size=`. At `font-size=0`, the default, the frit's `size=` decides, and with none vtium picks a size from the display, such as 8x16 cells at 640x480. A bitmap font draws at its own cell or a whole multiple of it, since points do not apply to it.

### Choosing a font with vtium config

`vtium config tui`, run inside a vtium console, opens a full-screen tool that lists the frits and the built-in fonts. Each font you move the highlight to becomes the console's font at once, so you judge the real console in it:

```
$ vtium config tui
```

Up and Down move the highlight, Enter keeps the highlighted font for saving, Tab moves to the sizes, to vtium.conf's font keys and to the heads, `s` shows what a save changes, and `q` or Ctrl-C leaves and puts back the font the console had when the tool started. Where the console draws kitty graphics, a vtium built with the `images` feature, each size shows a picture of the font at that size, and the console's own font and grid stay as they were. Without pictures, a size shows only once it is saved.

Saving lists each line of vtium.conf that changes and the one command it runs, `sudo /usr/bin/vtium-config apply DRAFT`, which asks for your password once. Run as root, vtium-config takes no option that names a directory or a program, so that command can change only `/etc/vtium/vtium.conf`, `/etc/vtium/heads.conf` and the frits in `/etc/vtium/fonts`, each after vtium's own checks have accepted it, and you can allow exactly that command in a sudo rule. The command checks the changed files with `vtium check-config`, installs them, and keeps each old file beside the new one as `vtium.conf.bak` or `heads.conf.bak`; it changes only the lines that need it, so your comments stay. A font saved this way, like any change to vtium.conf, takes effect at the console's next start, and the console you saved from stays in the saved font meanwhile.

In a terminal that is not vtium, such as a desktop terminal, the tool shows the same lists and saves the same way, but sends the terminal no font, and says so. With `allow-font-ops` off, it says so and offers to turn the key on as part of the save.

### Switching and zooming

`vtium config set NAME` switches the console it runs in to the frit NAME, or to a built-in font, `unifont` or `unscii-8`:

```
$ vtium config set monoid
```

It writes xterm's font sequence, OSC 50, to its own terminal, and vtium takes it while the `allow-font-ops` key is on, as it is by default.

`vtium config set` first asks the terminal what it is, and sends nothing to a terminal that is not vtium, such as a desktop terminal; there it prints `failed`. Run it inside a vtium console.

Ctrl+= and Ctrl+- zoom the console on screen in and out, and so does Ctrl with the mouse wheel.

A font switch and a zoom last until you log out. When the session in a console ends, vtium reads `font-name`, `font-size` and `font-engine` from `vtium.conf` again and returns that console to the font and size they give before the next login prompt appears. So a font you saved with `vtium config` is the font the next login sees, and a font you only tried is gone. When a screen has several consoles, logging out of one console returns the font only if the switch came from that console, or if it is the only console left. Closing the console the switch came from with Ctrl+Super+Backspace returns the font too, and so does vtium hanging up a screen's sessions when its last display goes under `when-display-lost = hangup`. A font key that vtium cannot read is logged and skipped, as at the start. vtium reads the other keys of `vtium.conf` only when it starts again. Set `font-name` and `font-size` ([Size](#size)) for a font and size that last.

## Usage

Sessions in vtium have `TERM=xterm-vtium`. Most remote hosts lack that terminfo entry, so read [ssh from vtium](../crates/vtium/README.md#ssh-from-vtium) before you ssh from vtium.

vtium keeps these keys for itself. A "head" is a terminal for one or more displays, and a "console" is one session within a head, with its own login, screen and scrollback.

| Keys | What vtium does |
|---|---|
| Ctrl+Alt+F1 to F12 | switches the VT |
| Shift+PageUp, Shift+PageDown | scrolls the view a page through the scrollback |
| Shift+Up, Shift+Down | scrolls the view a line |
| Ctrl+Shift+Up, Ctrl+Shift+Down | moves the view to the shell prompt above or below, where the shell marks its prompts with OSC 133 |
| Ctrl+=, Ctrl+- | zooms the console in and out |
| Super+=, Super+- | rotates the head clockwise and counterclockwise |
| Shift+Insert, the middle button | pastes the selection |
| Ctrl+click on a link | copies the link's address into the selection; vtium opens nothing |
| Ctrl+Super+Right, Ctrl+Super+Left | shows the head's next or previous console |
| Ctrl+Super+Return | opens a new console in the head |
| Ctrl+Super+Backspace | closes the console on screen, unless it is the head's last |
| Ctrl+Super+Up, Ctrl+Super+Down | makes the next or previous head active, from a keyboard that roams among heads |

Selecting text with the pointer copies it into vtium's selection. The console has no system clipboard: vtium holds one selection, and a program can set it with OSC 52, but no program can read it back. Under a head file whose policy is `closed`, Ctrl+Super+Return and Ctrl+Super+Backspace do nothing, so each head keeps the consoles it started with.

No key reboots, powers off or suspends the machine until you bind one with `grab-reboot`, `grab-power-off` or `grab-suspend` ([config.md](config.md), [systemd.md](systemd.md)). `grab-paste` changes the paste keys.

## Tips and tricks

### The development VT

`make vt-install VT=N` installs a development build from your checkout as `vtium-dev@ttyN`, beside the packaged vtium, on VT N in the place of its getty, and keeps it there across reboots. Run it as yourself; it asks sudo for each step that needs root. `DRY_RUN=1` prints what it would do and runs nothing:

```
$ make vt-install VT=5 DRY_RUN=1
$ make vt-install VT=5
$ make vt-uninstall VT=5
```

Run `make vt-install VT=5` again after a change to replace the build and restart `vtium-dev` on VT 5. It refuses VT 1, a VT outside 2 to 12, the VT on screen, a VT someone is logged in on, and a VT the packaged vtium or `kmscon` runs on or is enabled on. The development build reads the same `/etc/vtium` as the package. [vt-review.md](vt-review.md#installing-vtium-on-a-vt) describes it in full, with the maintainer's guided review of a build on a VT.

### Sharing VTs with kmscon

`kmscon` and vtium can each keep VTs of their own on one machine, through a VT's own `autovt@ttyN.service` link, which wins over the template link. To give `kmscon` a VT that vtium holds, VT 3 here, take VT 3 back from vtium first, then link VT 3 to `kmscon`:

```
# vtium console disable 3
$ vtium console status
# systemctl disable getty@tty3.service
# ln -s /usr/lib/systemd/system/kmsconvt@.service /etc/systemd/system/autovt@tty3.service
# systemctl daemon-reload
```

`disable 3` gives VT 3 back the getty it had ([Giving vtium a VT](#giving-vtium-a-vt)). Run the `systemctl disable getty@tty3.service` line only where `status` then shows a getty at boot on VT 3, since a getty that systemd starts at boot holds the VT, and logind then starts nothing there. The link keeps logind from starting vtium on VT 3 through the template link after `enable --all`, and it gives VT 3 to `kmscon` whatever the template link names. logind starts `kmscon` the next time you switch to VT 3 while nothing runs there, and a vtium that still runs on VT 3 keeps the VT until you stop it. [crates/vtium/README.md](../crates/vtium/README.md#sharing-vts-with-kmscon) gives the way back, and the other direction.

### Blanking from a script

`vtium blank --vt N` turns off the displays of the vtium on VT N at once, and `vtium wake --vt N` turns them on again, as a key press does. Root and the user vtium runs as may send them:

```
# vtium blank --vt 3
```

## Troubleshooting

### Getting out

When vtium misbehaves on a VT, these steps get you out, in this order:

1. Switch to another VT, or to your desktop's, with Ctrl+Alt+F1 to F12. On a VT where vtium runs, vtium reads the keyboard itself and asks the kernel for the switch, since the kernel's own handling of Ctrl+Alt+F1 to F12 is off there. So a vtium that no longer answers keys keeps you on its VT. Then take the next steps from another machine over ssh.
2. Restart that VT's vtium, from another VT, a desktop terminal or another machine over ssh. A restart ends every session on the VT. On VT 3:

   ```
   # systemctl restart vtium@tty3.service
   ```

   The development install's unit is `vtium-dev@tty3.service` ([The development VT](#the-development-vt)).
3. Take vtium off the VT at once by taking the VT back and starting its getty there:

   ```
   # vtium console disable 3
   # systemctl start getty@tty3.service
   ```

   `disable` decides what starts on VT 3 from now on ([Giving vtium a VT](#giving-vtium-a-vt)). Starting the getty stops vtium on VT 3, because vtium's unit conflicts with the getty's, and that ends every session there. On the development VT, `vtium console disable` refuses, since the development install holds the VT, and the `systemctl start getty@tty5.service` line alone stops `vtium-dev` there; `make vt-uninstall VT=5` takes the install off for good ([The development VT](#the-development-vt)).

When vtium fails, systemd starts it again. From systemd 254 on, each failure also starts vtium's fallback, which starts the VT's getty unless the head file's policy is `closed`, so under an open policy you get a plain login prompt at the first failure instead of a dead screen ([A VT that shows a getty](#a-vt-that-shows-a-getty)).

### Reading vtium console status

`vtium console status` first says which console logind starts on a VT with no link of its own, and which display manager the machine has. Then it prints a line for each VT:

- **at boot** names what systemd starts on the VT at boot, such as `getty@tty1`, `vtium@tty1` or `display manager (sddm.service)`. `getty recorded` means that `vtium console` disabled the VT's getty and keeps a record of it, which `disable` puts back. `no getty recorded` means that `vtium console` recorded that no getty was enabled on the VT, so `disable` enables none there. `vtium@ttyN masked` means someone masked vtium's unit there.
- **when switched to** names what logind starts when you switch to the VT while nothing runs there. It shows `-` on VT 1 and on VTs 7 and above, where logind starts no console ([Giving vtium a VT](#giving-vtium-a-vt)).
- **running** names what runs on the VT now, a graphical session included.

A VT whose **at boot** column names a getty keeps that getty when you switch to it, whatever the **when switched to** column says, because the getty already runs there.

### A font that does not load

vtium draws its built-in font when it cannot draw the frit, and logs why. Read the log of that VT's vtium:

```
# journalctl -b -u vtium@tty2.service
```

A line such as `the font monoid is not drawn (...)`, or `the font's glyphs are not generated (...)`, gives the reason. Check these in turn:

- vtium finds a frit by its file's name: `font-name=monoid` reads `/etc/vtium/fonts/monoid.vtfrit`. A file named otherwise, such as `Monoid.vtfrit`, is not found, and vtium then looks for a kbd font named `monoid`.
- A source the helper cannot find is skipped without a warning, and the next source draws in its place. When everything draws in the emoji font or the built-in font, the helper did not find your font: it searches only `/usr/share/fonts` and `/usr/local/share/fonts`, as user `nobody` ([An outline font](#an-outline-font)).
- The font file's own name table must give the family the frit names, as its typographic or its legacy family name. A file whose file name matches the family but whose name table names another family is not taken. `fc-scan --format '%{family}\n' FILE` prints the families a file gives.
- An unknown key or a malformed line refuses the whole frit, and the log names the file and the line.
- A change to the frit or to `vtium.conf` waits for vtium's next start on that VT ([Configuration](#configuration)).

`vtium check-config` also reports a frit whose `cell=` line vtium cannot read.

### A VT that shows a getty

The unit's fallback, `vtium-getty@ttyN.service`, starts a getty on a VT where vtium fails, so that you are never left at a dead screen. A login prompt where you expected vtium therefore usually means vtium failed. vtium exits with a failure, for example, when no display appears within `display-wait`, 10 seconds by default. Find the reason in its unit's state and log:

```
$ systemctl status vtium@tty2.service
# journalctl -b -u vtium@tty2.service
```

A getty that ran on the VT before you gave it to vtium also keeps the VT until it stops. Log out there and start vtium, as in [Giving vtium a VT](#giving-vtium-a-vt).

### A VT that shows nothing

- vtium turns its displays off after `dpms-timeout`, 600 seconds by default, with no key press or pointer action. Press a key.
- Under a head file whose policy is `closed`, the fallback starts no getty, since a getty's login would take any keyboard, and vtium waits for a display however long it takes. Run `vtium check-config` and read the log.
- A VT whose link still names `vtium@.service` after the package was removed gets no console at all. The [vtium-arch README](https://github.com/vtium/vtium-arch#removing-vtium) gives the commands that clean such links up.

### A VT that stays black after you switch back to it

You switch from the desktop back to a VT where vtium runs, and the screen stays black, and a key press does not bring it back. This is a known defect, which [issue #249](https://github.com/vtium/vtium/issues/249) follows. Until it is fixed, restart that VT's vtium ([Getting out](#getting-out)). The restart ends every session on the VT. If the screen stayed black until you rebooted, collect a report of the boot before with `vtium-config report --previous-boot --save` ([Collecting a report and filing an issue](#collecting-a-report-and-filing-an-issue)).

### Pictures that do not show

A program that draws sixel or kitty graphics pictures shows nothing, or shows text where the picture should be. Check these in turn:

- vtium draws pictures only in a build with the `images` feature. `vtium --version` names `images` on its `features` line for such a build, and so does the vtium part of `vtium-config report` ([Collecting a report and filing an issue](#collecting-a-report-and-filing-an-issue)). The Arch recipes build one with `VTIUM_FEATURES=images` ([Arch Linux](#arch-linux)).
- `image-storage-limit` must be above 0, since `0` turns pictures off, and neither `sixel` nor `kitty-graphics` may be turned off ([config.md](config.md)).
- The program must see that the terminal draws pictures. vtium tells a program that asks: its reply to DA1, the terminal's device attributes query, names sixel, and it answers kitty's graphics query. Some programs do not ask, and decide from `TERM` alone, which is `xterm-vtium` in vtium, so they offer pictures only to a terminal they know by name. Tell such a program which kind of picture to draw where it has an option for it, as `chafa -f kitty` does.
- A kitty graphics picture sent as a file, as shared memory or as PNG needs the image helper, `/usr/bin/vtium-image`, which a build with images installs. When vtium cannot start the helper, its log says why ([A font that does not load](#a-font-that-does-not-load) shows how to read the log).

### vtium console refuses a VT

Each refusal names what holds the VT and how to free it: `systemctl disable --now kmsconvt@ttyN.service` for `kmscon`, `make vt-uninstall VT=N` for the development install, `systemctl unmask vtium@ttyN.service` for a masked unit. vtium never takes a VT from a display manager or a graphical session; give vtium another VT.

### Collecting a report and filing an issue

`vtium-config report` gathers what a bug report needs into one document: the vtium builds installed and the features each was built with, the kernel and the distribution, the state of each vtium console, the displays the kernel sees, your configuration, and this boot's log of each vtium console. It changes nothing on the machine. Save it to a new file in the current directory:

```
$ vtium-config report --save
```

It prints the file's name, such as `vtium-report-20261103-203044.txt`. `--save=PATH` saves it elsewhere, and without `--save` it prints the document to the terminal. When the defect forced a reboot, its log is in the boot before this one, and `--previous-boot` reads that boot's log in place of this one's.

Run as your user, the report marks each part that only root can read, such as the kernel's display state, and says so when your user cannot read the system's journal. Run it through sudo to fill those parts in. Run as root, the report writes no file and refuses `--save`, so let your own shell save what it prints, as you:

```
$ sudo vtium-config report > vtium-report.txt
```

The report leaves out the machine's hostname and each monitor's serial number. It can still name user accounts: a login writes the user's name to the log, `vtium console status` names the user of each graphical session, and a line of `vtium.conf` can hold a user's name or home directory. The logs' times also show your time zone. Read the report before you share it, and remove anything you would rather keep to yourself.

File the issue at [the issue tracker](https://github.com/vtium/vtium/issues): say what you did, what you saw and what you expected, and attach the report.

## See also

- [crates/vtium/README.md](../crates/vtium/README.md#setting-it-up): the full rules of `vtium console`, sharing VTs with `kmscon`, and ssh from vtium
- [config.md](config.md): every configuration key
- [heads.md](heads.md): the head file, with an example for each setup
- [systemd.md](systemd.md): where vtium touches systemd, logind or PID 1, and how it runs without them
- [concepts.md](concepts.md): vtium's pieces and how they fit together
- [vt-review.md](vt-review.md): the maintainer's guided review of a build on a VT, and the development install
- [vtium-arch](https://github.com/vtium/vtium-arch): the Arch Linux recipes
- [The issue tracker](https://github.com/vtium/vtium/issues)
