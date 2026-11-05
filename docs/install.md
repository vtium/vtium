# Install vtium and give it VTs

This guide installs vtium and gives it the VTs you choose. A VT, or virtual terminal, is one of the full-screen text screens the Linux kernel keeps beside a desktop, and vtium runs on a VT in the place of the getty, the program that prints the login prompt there. If you have never run vtium, [Try vtium on one VT](tutorial-try-vtium.md) walks through a first install on one VT and back again.

In the commands below, `#` marks a command you run as root, for example with sudo, and `$` a command you run as your own user.

## Arch Linux

The AUR (the Arch User Repository) holds vtium's recipes. A recipe is a `PKGBUILD`, the build script that makepkg reads to build a package. Two console packages are published today. They conflict, so install one:

| Package | Built from |
|---|---|
| `vtium` | a release, the tag the maintainer signs, today the pre-release `v0.1.0-alpha.5` |
| `vtium-git` | the tip of vtium's `main` branch |

Each recipe also builds a terminfo package, `vtium-terminfo` beside `vtium` and `vtium-terminfo-git` beside `vtium-git`, which installs `xterm-vtium`, the terminfo entry that sessions in vtium name in `TERM`. The console package depends on it.

A recipe builds the console with the features that `VTIUM_FEATURES` names in makepkg's environment. The `images` feature draws sixel and kitty graphics pictures, the `logind` feature lets the suspend key ask logind, systemd's login manager, to suspend the machine, and the `gpu` feature builds in the path that draws through the GPU. When `VTIUM_FEATURES` is unset, both recipes build `images logind gpu`, for an ordinary desktop or laptop. To build another set of features from a checkout, stage it with `make install` ([From a checkout](#from-a-checkout)).

Both recipes build with `HWACCEL=off DISPLAY_BUFFERS=3`, which compiles in `hwaccel` off and three buffers for each display as the defaults ([config.md](config.md)). A console built with `gpu` therefore draws on the CPU until `hwaccel = on` in `/etc/vtium/vtium.conf` turns the GPU path on. Both depend on `libglvnd` and `mesa`, which hold the two libraries the GPU path loads, `libEGL.so.1` and `libgbm.so.1`.

No package ships a font. The console draws with its built-in fonts, unifont and unscii-8. Both packages install three frits (font set files) that name fonts the distribution packages, `desktop`, `minspec-16` and `minspec-8`. A frit leaves out a font the machine lacks, so a frit with none of its fonts installed draws the built-in fonts. On Arch the frits' fonts are in `ttf-jetbrains-mono`, `ttf-nerd-fonts-symbols-mono` and `noto-fonts-emoji`.

makepkg refuses to build the `vtium` recipe until your keyring holds the maintainer's signing key, `33F9B54FD2A4B6A4110749B3BA464FC9847A832A`. The recipe carries a copy of the key, so import it once from there, then build and install. The example builds pictures, the power keys and the GPU path, the features the recipe builds by default:

```
$ git clone https://aur.archlinux.org/vtium.git
$ cd vtium
$ gpg --import keys/pgp/33F9B54FD2A4B6A4110749B3BA464FC9847A832A.asc
$ VTIUM_FEATURES='images logind gpu' makepkg -si
```

`gpg --recv-keys 33F9B54FD2A4B6A4110749B3BA464FC9847A832A` fetches the same key from a keyserver instead. `vtium-git` checks no signature, because GitHub signs the merge commits on `main` with its own key.

The packages install these files:

| File | What it is |
|---|---|
| `/usr/bin/vtium` | the console, with its subcommands `vtium console`, `vtium check-config`, `vtium config`, `vtium blank` and `vtium wake` |
| `/usr/bin/vtium-config` | the font and settings tool, which `vtium config` runs: `vtium config tui` opens a tool that tries each font on the live console and saves your choice into vtium.conf, and `vtium config set` switches the font; it is also the glyph helper, which draws a font's glyphs for the console |
| `/usr/lib/systemd/system/vtium@.service` | the unit that runs vtium on one VT in place of its getty |
| `/usr/lib/systemd/system/vtium-getty@.service` | the fallback that starts a getty on the VT when vtium fails |
| `/usr/bin/vtium-image` | the image helper, which reads a picture a program names by its file, in a console built with `images` |
| `/usr/lib/sysusers.d/vtium-image.conf` | the image helper's account, in a console built with `images` |
| `/usr/bin/vtium-gallery` | the picture gallery tool, which shows PNG pictures as a grid of thumbnails, in a console built with `images` |
| `/etc/vtium/vtium.conf.example` | the example configuration, which lists every key at its default, commented out |
| `/etc/vtium/fonts/` | the frits `desktop`, `minspec-16` and `minspec-8`; each is ready to choose with `vtium config` or vtium.conf's `font-name`, and a frit that nothing names changes nothing |
| `/usr/share/doc/vtium/examples/` | an example udev rule and hwdb fragment, which are documentation and never active, and `heads.conf`, the head file's commented template, to copy to `/etc/vtium/heads.conf` for a terminal of its own on each display |
| `/usr/share/terminfo/x/xterm-vtium` | the terminfo entry, from the terminfo package |
| `/usr/share/licenses/<package>/LICENSE` | the license, in each package |

Installing the packages gives vtium no VT and starts nothing. [Giving vtium a VT](#giving-vtium-a-vt) does that.

### The packages to come

The recipe repository is to change the two packages it publishes and to publish more, as [ADR-1500](architecture/project/ADR-1500-vtium-is-packaged-from-a-recipe-repo-per-package-manager-under-names-of-its-own.md) D8 sets out. None of this is published yet:

- A later release of `vtium` and `vtium-git` turns `hwaccel` on by default.
- `vtium-minspec` and `vtium-minspec-git` are to build the minspec console with no feature, for a small machine such as a single-board computer, a server's console or a kiosk.
- `vtium-minspec-gpu` and `vtium-minspec-gpu-git` are to build the minspec console with the GPU path and leave `hwaccel` off, with `libglvnd` and `mesa` as optional dependencies.
- The minspec packages are to keep two buffers for each display, where `vtium` keeps three ([config.md](config.md), `display-buffers`).
- `vtium` is to ship a fourth frit, for the Terminus console font, and an optional package, `vtium-fonts`, is to depend on every font the shipped frits name: on Arch `ttf-jetbrains-mono`, `ttf-nerd-fonts-symbols-mono`, `noto-fonts-emoji` and `terminus-font`.

## From a checkout

Building vtium needs Rust 1.88 or later and clang. The checkout's `rust-toolchain.toml` names the toolchain the tests run on, and rustup installs it for you inside the checkout. The first build fetches a Debian sysroot, a copy of a Debian system's libraries with glibc 2.36, to link against, so the binaries run on any distribution with that glibc or a newer one ([tools/arm/README.md](https://github.com/vtium/vtium/blob/main/tools/arm/README.md)).

On a machine with systemd and logind, `make vt-shell` builds the console, picks a free VT, asks for sudo, and runs your own shell in vtium there until you exit it. It installs nothing; [the tutorial](tutorial-try-vtium.md#without-a-package) walks through it, and [vt-review.md](vt-review.md) describes it and the other runs on a VT that a checkout offers:

```
$ make vt-shell
```

To install a branch of your own as a package, point the `vtium-git` recipe at your checkout. makepkg builds the branch's last commit, so commit first:

```
$ VTIUM_GIT_URL=file:///home/me/src/vtium VTIUM_GIT_BRANCH=my-branch makepkg -si
```

On another distribution, `make install DESTDIR=DIR PREFIX=/usr` stages under DIR exactly the files a console package ships, and `make install-terminfo DESTDIR=DIR PREFIX=/usr` stages the terminfo entry, so that a package build can call them. `make install` builds the console with the features `FEATURES='...'` names, none by default, and stages the frits `FRITS='...'` names, by default `desktop`, `minspec-16` and `minspec-8`. `HWACCEL=on` or `off` and `DISPLAY_BUFFERS=1`, `2` or `3` set the defaults of `hwaccel` and `display-buffers` that the build compiles in. The Arch recipes name all three. Both refuse an empty DESTDIR and `/`, so neither installs onto the machine itself. The [vtium-arch README](https://github.com/vtium/vtium-arch) describes how the Arch recipes use them.

`make install-tree` stages the same programs under `target/install-tree`: the console, `vtium-config`, the VT unit and its getty fallback, the terminfo entry, the example configuration, the frits and the head file's template. It also needs `tic`, which compiles the terminfo entry. Tier 3, vtium's test harness on a virtual machine, installs that tree in its guest.

## Giving vtium a VT

`vtium console` gives a VT to vtium, or every VT, and takes them back. It changes which console systemd starts on a VT at boot, and which console logind starts when you switch to a VT where nothing runs. Each call of `enable` or `disable` takes one VT by its number, or `--all`. `enable` and `disable` need root, and `status` runs as any user.

See first what each VT runs now:

```
$ vtium console status
```

[Reading vtium console status](troubleshooting.md#reading-vtium-console-status) explains its columns.

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

- a VT another program uses now or will claim at the next boot (below);
- a VT where `kmscon` is enabled, linked or running, and a VT the development install holds ([The development VT](vt-review.md#installing-vtium-on-a-vt));
- a VT whose own link, `/etc/systemd/system/autovt@ttyN.service`, names another console;
- VTs above 6, where logind starts no console by itself; `enable N` gives vtium such a VT at boot.

VT 6 is logind's reserved VT, where logind starts a console at every switch, even while the VT is in use. `enable 6` and `enable --all` refuse while someone is logged in on VT 6, unless that login is inside a vtium running there. A getty that waits at its login prompt on VT 6 holds no session, so both go ahead, and the next switch to VT 6 stops that getty. Where VT 6 has no link of its own, `enable --all` also refuses, rather than passing VT 6 by, while `kmscon` or the development install is enabled or running there.

### VTs that other programs hold

`vtium console` follows common convention: on a machine with a display manager, VT 1 stays the display manager's and its getty's, as systemd's preset and the display managers' own units leave it, and vtium takes other VTs. It never takes a VT that your system is set up to give to another program: a unit enabled at boot that claims the VT, or an enabled display manager. What is running now on a VT holds it as well, but never frees one. `enable N` refuses such a VT, `enable --all` passes it by, and both say what holds the VT and how vtium knows:

- VT 1, while a display manager is enabled, whatever VT the display manager uses now: `/etc/systemd/system/display-manager.service` links to it, or a unit that starts at boot runs one vtium knows by name, such as ly's `ly@tty2`, which ly enables without that link;
- a VT whose getty a unit that starts at boot names in `Conflicts=`, as Arch's `sddm.service` names `getty@tty1.service`;
- a graphical session or a greeter on the VT now, which it asks logind for;
- another console running on the VT now, such as `fbterm@tty4.service`;
- a unit that starts at boot and runs on the VT, such as `cage@tty3.service` or a unit with `TTYPath=/dev/tty9`;
- a VT your display manager's own configuration starts its greeter on: LightDM's `minimum-vt`, 7 unless you set it, greetd's `vt`, and SDDM's `MinimumVT`.

Nothing frees a VT: a display manager's greeter running on VT 2 leaves VT 1 the display manager's all the same. `vtium console status` gives the reasons under its table.

If vtium fails on a VT, its fallback starts that VT's getty again, so that you have a login. It never does this when the getty's start would stop another program, as starting `getty@tty1` would stop a display manager whose unit conflicts with it. Then the VT is left without a console, and `journalctl -b -u vtium-getty@tty1.service` says why. Log in on another VT or over ssh to find out why vtium failed ([A VT that shows a getty](troubleshooting.md#a-vt-that-shows-a-getty)).

On a machine where another terminal, such as `kmscon`, holds the template link or runs on your VTs, read [Moving from another DRM/KMS VT](#moving-from-another-drmkms-vt) before you run `enable --all`.

Before `enable` changes anything, it records what each VT had in `/var/lib/vtium/console/`, and `disable` puts back exactly that:

```
# vtium console disable 3
# vtium console disable --all
```

`disable 3` takes vtium off VT 3's boot list, removes VT 3's own link to vtium, and gives VT 3 back the getty it had. After `enable --all`, though, logind still starts vtium on VT 3 through the template link whenever you switch to VT 3 while nothing runs there, and `disable 3` says so. Only `disable --all` points the template link back where it pointed before.

[crates/vtium/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#setting-it-up) gives the full rules. On a machine that was not booted with systemd, `vtium console` changes nothing and says so; there you replace a VT's getty entry with one that runs `vtium --vt=ttyN --no-switchvt`, the way your init system runs the getty, as [Giving vtium a VT without systemd](systemd.md#giving-vtium-a-vt-without-systemd) shows for sysvinit, OpenRC and runit.

## Moving from another DRM/KMS VT

A terminal that draws on a VT through DRM/KMS, as vtium does, such as `kmscon`, starts there in one of two ways: from a unit enabled at boot for that VT, or from the template link `/etc/systemd/system/autovt@.service`, which logind follows when you switch to a VT that nothing runs on. `vtium console` passes by every VT where another terminal is enabled at boot or still runs ([VTs that other programs hold](#vts-that-other-programs-hold)), so take those VTs back first. Do it from a desktop terminal or over ssh, after logging out of each VT where the other terminal runs. With `kmscon`, whose unit is `kmsconvt@.service`:

```
# systemctl disable kmsconvt@.service
$ systemctl list-units 'kmsconvt@*'
# systemctl stop kmsconvt@tty2.service
# vtium console enable --all
```

Run the `stop` line for each VT that `list-units` shows. For another terminal, put its own unit's name in place of `kmsconvt@`. `systemctl disable` stops nothing, and logging out of a VT leaves the terminal running there. `vtium console enable --all` refuses while the template link names `kmscon`'s unit. When the template link names another terminal's unit, `enable --all` records the link before it points it at vtium, and `disable --all` points it back.

Removing `kmscon` afterwards is up to you, with `pacman -R kmscon`; vtium needs nothing from it. vtium never reads `/etc/kmscon/kmscon.conf`. vtium takes the keys `kmscon` used, so you can copy that file to `/etc/vtium/vtium.conf` by hand and check it with `vtium check-config`; [config.md](config.md) lists every key vtium reads. [crates/vtium/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#sharing-vts-with-kmscon) says when to keep `kmscon-terminfo`.

## Upgrading and removing

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

`disable --all` puts back what each VT had and then forgets its record, so `/var/lib/vtium/console/` is left empty, and you can remove it. The [vtium-arch README](https://github.com/vtium/vtium-arch#removing-vtium) says what to do when you removed the package first.

## Sharing VTs with kmscon

`kmscon` and vtium can each keep VTs of their own on one machine, through a VT's own `autovt@ttyN.service` link, which wins over the template link. To give `kmscon` a VT that vtium holds, VT 3 here, take VT 3 back from vtium first, then link VT 3 to `kmscon`:

```
# vtium console disable 3
$ vtium console status
# systemctl disable getty@tty3.service
# ln -s /usr/lib/systemd/system/kmsconvt@.service /etc/systemd/system/autovt@tty3.service
# systemctl daemon-reload
```

`disable 3` gives VT 3 back the getty it had ([Giving vtium a VT](#giving-vtium-a-vt)). Run the `systemctl disable getty@tty3.service` line only where `status` then shows a getty at boot on VT 3, since a getty that systemd starts at boot holds the VT, and logind then starts nothing there. The link keeps logind from starting vtium on VT 3 through the template link after `enable --all`, and it gives VT 3 to `kmscon` whatever the template link names. logind starts `kmscon` the next time you switch to VT 3 while nothing runs there, and a vtium that still runs on VT 3 keeps the VT until you stop it. [crates/vtium/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#sharing-vts-with-kmscon) gives the way back, and the other direction.
