# Install vtium and give it VTs

This guide shows you how to install vtium. It says which VTs the package gives vtium, and how you choose other VTs. A VT, or virtual terminal, is one of the full-screen text screens that the Linux kernel keeps beside a desktop. vtium runs on a VT in place of the getty, the program that prints the login prompt there. If you have never run vtium, start with [Try vtium on one VT](tutorial-try-vtium.md). It walks you through a first install on one VT, and back again.

In the commands below, `#` marks a command that you run as root, for example with sudo. `$` marks a command that you run as your own user.

## Arch Linux

The AUR (the Arch User Repository) carries vtium's packages for Arch. Each package is built from a recipe, a `PKGBUILD`, which is the build script that makepkg reads. The repository `vtium/vtium-arch` keeps nine recipes, one for each package. The AUR carries five of them. The console packages conflict with each other, so you install only one of them:

| Package | What it builds | Built from | On the AUR |
|---|---|---|---|
| `vtium` | the console for an ordinary desktop or laptop | a release: the tag the maintainer signs, the latest on [the releases page](https://github.com/vtium/vtium/releases) | yes |
| `vtium-git` | the same console | the tip of vtium's `main` branch | yes |
| `vtium-minspec` | vtium's smallest console, without images and without the GPU path | the same release tag | no: build it from the `vtium` recipe ([below](#build-the-minspec-console)) |
| `vtium-minspec-git` | the same console | the tip of `main` | no: build it from the `vtium-git` recipe |
| `vtium-minspec-gpu` | the smallest console with the GPU path | the same release tag | no: build it from the `vtium` recipe |
| `vtium-minspec-gpu-git` | the same console | the tip of `main` | no: build it from the `vtium-git` recipe |
| `vtium-terminfo` | `xterm-vtium`, the terminfo entry that sessions in vtium name in `TERM` | the same release tag | yes |
| `vtium-terminfo-git` | the same entry | the tip of `main` | yes |
| `vtium-fonts` | a curated set of fonts chosen to go with vtium's frits (font set files). It installs them through the distribution's own font packages. | the recipe repository alone | from the next publish |

Each console package depends on a terminfo package. A console built from the release tag depends on `vtium-terminfo`. A console built from `main` depends on `vtium-terminfo-git`. Today the AUR builds `vtium-terminfo` from the `vtium` recipe, and `vtium-terminfo-git` from the `vtium-git` recipe. From the next publish, each terminfo package has a recipe of its own on the AUR.

A recipe builds the console with the features that `VTIUM_FEATURES` names in makepkg's environment. There are five features:

- The `images` feature renders sixel and kitty graphics images.
- The `logind` feature lets the suspend key ask logind, systemd's login manager, to suspend the machine.
- The `gpu` feature builds in the path that renders through the GPU.
- The `bidi` feature lays out right-to-left text, such as Arabic and Hebrew.
- The `libseat` feature lets vtium run in seat mode, where it gets the screen, the keyboards and VT switching from a seat manager through libseat. [Direct mode and seat mode](#direct-mode-and-seat-mode) describes seat mode.

When `VTIUM_FEATURES` is unset, the `vtium` and `vtium-git` recipes build `images logind gpu bidi libseat`, for an ordinary desktop or laptop. To build another set of features from a checkout, stage it with `make install`. See [From a checkout](#from-a-checkout) for details.

From v0.1.0-alpha.6 on, the `vtium` and `vtium-git` recipes build with `HWACCEL=on DISPLAY_BUFFERS=3`. These settings compile in two defaults: `hwaccel` is on, and each display gets three buffers. See [config.md](config.md) for both keys. The console therefore renders through the GPU path. vtium renders a head on the CPU when the GPU set-up for that head fails. A head is a terminal for one or more displays. A line `no-hwaccel` in `/etc/vtium/vtium.conf` turns the GPU path off.

v0.1.0-alpha.5 built with `HWACCEL=off`. Its console rendered on the CPU until a line `hwaccel` in `/etc/vtium/vtium.conf` turned the GPU path on.

`vtium` and `vtium-git` depend on `libglvnd` and `mesa`. These two packages hold the two libraries that the GPU path loads, `libEGL.so.1` and `libgbm.so.1`. They also depend on `seatd`, which holds `libseat.so`, the library that seat mode links. Installing `seatd` enables no seatd daemon.

No package ships a font. The console renders text in its built-in fonts, unifont and unscii-8. `vtium` and `vtium-git` install three frits (font set files): `desktop`, `minspec-16` and `minspec-8`. Each of them names fonts that the distribution packages. A frit leaves out any font that the machine lacks. So a frit with none of its fonts installed renders in the built-in fonts. On Arch, the frits' fonts are in `ttf-jetbrains-mono`, `ttf-nerd-fonts-symbols-mono` and `noto-fonts-emoji`.

makepkg refuses to build the `vtium` recipe until your keyring holds the maintainer's signing key, `33F9B54FD2A4B6A4110749B3BA464FC9847A832A`. The recipe carries a copy of the key. Import it once from there, then build and install. The example builds the features that the recipe builds by default: images, the power keys, the GPU path, right-to-left text, and seat mode.

```
$ git clone https://aur.archlinux.org/vtium.git
$ cd vtium
$ gpg --import keys/pgp/33F9B54FD2A4B6A4110749B3BA464FC9847A832A.asc
$ VTIUM_FEATURES='images logind gpu bidi libseat' makepkg -si
```

`gpg --recv-keys 33F9B54FD2A4B6A4110749B3BA464FC9847A832A` fetches the same key from a keyserver instead. `vtium-git` checks no signature, because GitHub signs the merge commits on `main` with its own key.

The packages install these files:

| File | What it is |
|---|---|
| `/usr/bin/vtium` | the console, with its subcommands `vtium console`, `vtium check-config`, `vtium config`, `vtium blank` and `vtium wake` |
| `/usr/bin/vtium-config` | the configuration tool, which `vtium config` runs. It keeps your changes in a draft, and installs the draft as `/etc/vtium/vtium.conf`. `vtium config tui` opens its full-screen view, where you can also try each font on the live console. `vtium config use-frit` switches the font. See [Configure vtium](configure.md) for details. The same program is also the glyph helper, which renders a font's glyphs for the console. |
| `/usr/lib/systemd/system/vtium@.service` | the unit that runs vtium on one VT in place of its getty |
| `/usr/lib/systemd/system/vtium-getty@.service` | the fallback that starts a getty on the VT when vtium fails |
| `/usr/bin/vtium-image` | the image helper, in a console built with `images`. It reads an image that a program names by its file. |
| `/usr/lib/sysusers.d/vtium-image.conf` | the image helper's account, in a console built with `images` |
| `/usr/bin/vtium-gallery` | the image gallery tool, in a console built with `images`. It shows PNG images as a grid of thumbnails. |
| `/etc/vtium/vtium.conf.example` | the example configuration. It lists every key at its default, commented out. Then it lists the heads and rules that vtium runs with when there is no configuration. |
| `/etc/vtium/fonts/` | the frits `desktop`, `minspec-16` and `minspec-8`. You can choose each one with `vtium config` or with vtium.conf's `font-name`. A frit that nothing names changes nothing. |
| `/usr/share/doc/vtium/examples/` | an example udev rule and hwdb fragment. They are documentation, and they are never active. |
| `/usr/share/terminfo/x/xterm-vtium`, `/usr/share/terminfo/v/vtium` | the terminfo entry, under its name and its alias `vtium`, from the terminfo package |
| `/usr/share/licenses/<package>/LICENSE` | the license, in each package |
| `/usr/share/licenses/<package>/COPYING` | vtium's authors, and the notices for the third-party code and data that vtium builds in. Every package carries it except `vtium-fonts`, which carries only its own `LICENSE`. |
| `/usr/share/licenses/<package>/OFL-1.1.txt`, `rgb-license.txt` | the two license texts that `COPYING` points to: the SIL Open Font License of the Unifont font, and the license of X.Org's color names. Each console package carries them. |

When you first install a console package, it gives vtium every free VT. It starts nothing. [What the install does to your VTs](#what-the-install-does-to-your-vts) explains what that means.

### Build the minspec console

The minspec console is vtium's smallest build. It suits a small machine, such as a single-board computer, a server's console, or a kiosk. The AUR carries no minspec package. You build the minspec console from the `vtium` recipe instead, or from the `vtium-git` recipe for the tip of `main`. To build the console with no features, set `VTIUM_FEATURES` and leave it empty. The console then has no images, no logind, no GPU path, and no seat mode:

```
$ cd vtium
$ VTIUM_FEATURES= makepkg -si
```

`VTIUM_FEATURES=gpu makepkg -si` builds the minspec console with the GPU path alone. `vtium --version` names the features that a console was built with, such as `features none` or `features gpu`.

`VTIUM_FEATURES` changes only the features. The package you build keeps everything else that the recipe sets: its dependencies on `libglvnd`, `mesa` and `seatd`, its frits, and its defaults for `hwaccel` and `display-buffers`. A line in `/etc/vtium/vtium.conf` changes either default. For example, `display-buffers=2` gives each display two buffers. See [config.md](config.md) for the keys.

The repository `vtium/vtium-arch` also keeps four minspec recipes: `vtium-minspec`, `vtium-minspec-gpu`, and their `-git` twins. Its CI builds them to check that the minspec console still builds.

### What the next publish adds

`vtium-fonts` reaches the AUR with the next publish. It is a curated set of fonts chosen to go with the frits (font set files) that vtium ships. It grows as new frits are added. It installs no font file itself. Instead, it depends on the distribution's package for each font that the frits name. On Arch these are `ttf-jetbrains-mono`, `ttf-nerd-fonts-symbols-mono`, `noto-fonts-emoji` and `terminus-font`. The console packages list `vtium-fonts` as an optional dependency, so you install it only if you want those fonts.

The `vtium` and `vtium-git` recipes in `vtium/vtium-arch` also ship a fourth frit, `terminus`, for the Terminus console font. The packages that the AUR carries today do not have it yet.

## From a checkout

Building vtium needs Rust 1.88 or later, and clang. The checkout's `rust-toolchain.toml` names the toolchain that the tests run on. rustup installs that toolchain for you inside the checkout. The first build fetches a Debian sysroot to link against. The sysroot is a copy of a Debian system's libraries, with glibc 2.36. Because vtium links against it, the binaries run on any distribution with that glibc or a newer one. See [tools/arm/README.md](https://github.com/vtium/vtium/blob/main/tools/arm/README.md) for details.

On a machine with systemd and logind, `make vt-shell` builds the console, picks a free VT, and asks for sudo. Then it runs your own shell in vtium on that VT, until you exit the shell. It installs nothing. [The tutorial](tutorial-try-vtium.md#without-a-package) walks through it. [vt-review.md](vt-review.md) describes it, along with the other runs on a VT that a checkout offers.

```
$ make vt-shell
```

To install a branch of your own as a package, point the `vtium-git` recipe at your checkout. makepkg builds the branch's last commit, so commit first:

```
$ VTIUM_GIT_URL=file:///home/me/src/vtium VTIUM_GIT_BRANCH=my-branch makepkg -si
```

On another distribution, a package build can call two make targets:

- `make install DESTDIR=DIR PREFIX=/usr` stages under DIR exactly the files that a console package ships.
- `make install-terminfo DESTDIR=DIR PREFIX=/usr` stages the terminfo entry.

`make install` builds the console with the features that `FEATURES='...'` names. By default it names none. `make install` also stages the frits that `FRITS='...'` names. By default these are `desktop`, `minspec-16` and `minspec-8`. Two more variables set defaults that the build compiles in. `HWACCEL=on` or `off` sets the default of `hwaccel`. `DISPLAY_BUFFERS=1`, `2` or `3` sets the default of `display-buffers`. The Arch recipes name all four of these variables: `FEATURES`, `FRITS`, `HWACCEL` and `DISPLAY_BUFFERS`. Both make targets refuse an empty DESTDIR, and they refuse `/`. So neither of them installs onto the machine itself. The [vtium-arch README](https://github.com/vtium/vtium-arch) describes how the Arch recipes use them.

Some package builds run with no network, as Debian's does. Such a build takes the crates that vtium builds with from the release:

- Each GitHub release from v0.1.0-alpha.6 on carries an archive, `vtium-VERSION-vendor.tar.zst`. VERSION is the release's version without its leading v. The archive's SHA-256 is in `vtium-VERSION-vendor.tar.zst.sha256`.
- The archive holds the source of every outside crate in `Cargo.lock`, for every architecture. It also holds a `.cargo/config.toml` that tells Cargo to read those crates from `vendor/`.
- Unpack the archive at the top of the release's source tree, beside `Cargo.toml`, with `tar -xf vtium-VERSION-vendor.tar.zst -C DIR`. It adds `vendor/` and `.cargo/config.toml`, and it replaces none of vtium's files. `make install` and `cargo build --offline --locked` then build with no network.

When a release is published, a CI workflow packs the archive. The workflow builds vtium offline from the archive before it attaches the archive to the release. In a checkout, `make vendor-archive` packs the same archive for the commit you have checked out. `make vendor-archive-check` also builds vtium offline from that archive.

With both `libseat` and `logind` in `FEATURES`, `make install` also stages a second unit, `vtium-seat@.service`, and the PAM file `/etc/pam.d/vtium`. These two files run vtium in seat mode, which [Direct mode and seat mode](#direct-mode-and-seat-mode) describes. See [systemd.md](systemd.md) for details.

`make install-tree` stages the same programs under `target/install-tree`: the console, `vtium-config`, the VT unit and its getty fallback, the terminfo entry, the example configuration, and the frits. It also needs `tic`, which compiles the terminfo entry. Tier 3 is vtium's test harness on a virtual machine. It installs that tree in its guest.

## What the install does to your VTs

When you first install a console package, its install script runs `vtium console enable --direct --all`. That command gives vtium every free VT. A free VT is one that no display manager, greeter, graphical session or `kmscon` holds. The script starts nothing, so each VT changes at its next start:

- On a machine with no display manager, vtium runs on VT 1 from the next boot, in place of its getty. VT 1 is where the kernel shows the boot messages. So vtium takes over the screen where the boot appeared.
- On a machine with a display manager, VT 1 stays the display manager's. vtium takes only the other free VTs.
- On each other free VT from 2 to 6, vtium also runs from the next boot, and waits at its login prompt there.

From boot, a vtium holds each of these VTs open. Some display managers put a new desktop session on the first VT that nobody holds open, as SDDM 0.20 and later does. Such a display manager passes vtium's VTs by, and uses VT 7 or above. Each of vtium's VTs runs one vtium from boot, and each vtium uses memory while it waits. See [the issue about the display manager that took vtium's VT](https://github.com/vtium/vtium/issues/370).

The install message shows what `vtium console` did. It names each VT that it left alone, and why. An upgrade changes no VT, so if you gave the VTs back, they stay back.

vtium may have taken your VTs in a release that did not yet start vtium at boot on each VT it takes. On such a machine, `vtium console status` names the VTs that nothing holds open at boot. Run `sudo vtium console enable --direct --all` again, and vtium then starts at boot on those VTs. [Giving vtium a VT](#giving-vtium-a-vt) shows how you change the choice.

`vtium console` refuses to change anything in three cases:

- while `kmscon` holds the template link `/etc/systemd/system/autovt@.service`. The template link names the console that logind starts on a VT with no link of its own.
- while `loginctl` cannot say which sessions run.
- on a machine not booted with systemd, such as a chroot that an installer prepares.

The install still succeeds. Its message then says that vtium took no VT, and gives the reason. It also names the command to run once the reason is gone:

```
# vtium console enable --direct --all
```

On a machine where `kmscon` holds the template link, read [Moving from another DRM/KMS VT](#moving-from-another-drmkms-vt) first.

### Direct mode and seat mode

vtium can run on a VT in two ways. The install turns on the first:

- In direct mode, vtium opens the screen and the keyboards itself, as root. Where seatd runs, a console built with `libseat` gets them from seatd instead. Each login at vtium's prompt gets an active logind session on seat0, just as a login at a getty does. So sound, removable disks, and the actions that polkit allows the active session all work as usual. The unit is `vtium@.service`.
- In seat mode, vtium gets the screen, the keyboards and VT switching from logind, through libseat. logind is the service that manages logins and seats. Each login gets a logind session with no seat. polkit then asks for an administrator's password for more actions, such as powering off. udev gives that session no devices, such as sound cards. The unit is `vtium-seat@.service`. Only a console built with both `libseat` and `logind` ships this unit. The `vtium` and `vtium-git` packages are built that way by default. The minspec packages are not, so they do not ship it. When you build from the `vtium` recipe with your own `VTIUM_FEATURES`, the package ships this unit only if `VTIUM_FEATURES` names both `libseat` and `logind`.

You can tell which mode a VT runs in from `vtium console status`, and from `loginctl`. See [What you see in each mode](concepts/seats-and-sessions.md#what-you-see-in-each-mode) for examples of both, and for what you see in seat mode.

A VT runs one mode at a time. To ask which mode fits your machine, run `vtium console enable` with no mode. It changes nothing. It says what it finds, which mode it recommends and why, and the command to run. When another program holds the VT you name, the advice names that program, and it offers no command for that VT. `vtium console status` shows you which VTs are free. To move every VT that vtium has to seat mode, and back:

```
# vtium console enable --seat --all
# vtium console enable --direct --all
```

`vtium console enable --seat` refuses, and changes nothing, in these cases:

- on a console built without `libseat` and `logind`;
- when `vtium-seat@.service` is missing or masked;
- when `/etc/pam.d/vtium` is missing;
- on a machine where logind manages no seat0.

VT 6 is logind's reserved VT. While vtium runs on VT 6 in one mode, `vtium console enable` does not move VT 6 to the other mode. The next switch to VT 6 would end its sessions. Instead, it tells you how to go ahead. See [the libseat issue](https://github.com/vtium/vtium/issues/34) for the work on seat mode.

### When vtium fails on a VT

Each VT that vtium takes keeps its own getty as the fallback. If vtium fails on a VT, the VT's fallback starts that VT's getty, so you have a login prompt there. [VTs that other programs hold](#vts-that-other-programs-hold) describes the one case where the fallback starts nothing.

vtium also steps aside for systemd's rescue and emergency modes, as a getty does. When systemd enters either mode, it first stops vtium on every VT. The rescue shell then asks for root's password on the VT that is shown.

### Turning vtium off for one boot

To start a machine once without vtium, add `vtium.enable=0` to the kernel command line in your boot loader. vtium then exits as soon as it starts, before it touches the screen. Under the open policy, each VT's getty fallback gives you a login prompt.

Under the closed policy, the key still turns vtium off, but no login prompt starts on vtium's VTs. A login prompt would accept any keyboard, which the closed policy forbids. If you run a closed vtium.conf, set a password on your boot loader, so that nobody else can edit the kernel command line. See [Turning vtium off for one boot](systemd.md#turning-vtium-off-for-one-boot) for what vtium does under each policy.

The key also works with or without systemd. Without systemd, your init system chooses what runs on the VT after vtium exits. With systemd, you can instead put `systemd.unit=rescue.target` on the kernel command line. That boots into rescue mode, and vtium steps aside for it.

**Note:** Masking vtium's unit, as with `systemd.mask=vtium@tty2.service`, does not turn vtium off on every VT. logind starts vtium through the template link `autovt@.service`. A mask does not reach a unit that systemd loads through a link. After the install, VT 1 also has no getty enabled. So a mask of `vtium@tty1.service` leaves VT 1 with no console.

### The kernel's messages

The kernel draws its own text on a VT through fbcon, the framebuffer console. vtium renders over fbcon on the VTs that it holds. It leaves fbcon to draw every other VT, so a getty and the rescue shell still show their text. fbcon draws nothing on vtium's VT, so vtium shows the kernel's messages itself. It shows each message from the kernel whose level is warning or more severe. It shows them on the boot display, over the console. It never writes them into a session's screen. `quiet` or `loglevel=3` on the kernel command line lets fewer messages through, and vtium then shows fewer too.

Plymouth, the boot splash program, shows the kernel's messages itself while it runs. The vtium.conf key `kernel-messages` decides who shows them then:

| Value | What vtium does |
|---|---|
| `after-plymouth` | the default: vtium leaves the messages to Plymouth while Plymouth runs, and shows them itself once Plymouth has quit. On a machine without Plymouth, vtium shows them from its start. |
| `always` | vtium shows the messages whether Plymouth runs or not. |

At a kernel panic, the kernel stops every program, vtium included. The screen then shows the kernel's own panic screen, on a kernel and a display driver that support one. [The boot issue](https://github.com/vtium/vtium/issues/266) tracks starting vtium earlier, from the initramfs, so that it shows the boot from its first second.

## Giving vtium a VT

The package's install has already run `vtium console enable --direct --all`. You can change that choice at any time with `vtium console`. It gives one VT or every VT to vtium, and it takes them back. It changes which console systemd starts on a VT at boot. It also changes which console logind starts when you switch to a VT where nothing runs. Each call of `enable` or `disable` takes one VT by its number, or `--all`. `enable` and `disable` need root. `status` runs as any user.

First see what each VT runs now:

```
$ vtium console status
```

[Reading vtium console status](troubleshooting.md#reading-vtium-console-status) explains its columns.

To give vtium only one VT, VT 3 in this example, first take every VT back with `disable --all`, as shown below. Then run:

```
# vtium console enable --direct 3
```

vtium is then VT 3's console from the VT's next start. systemd starts `vtium@tty3` there at boot, in place of the getty that `enable` disables. `enable` prints the command that starts vtium now. Log out of VT 3, and run that command from another VT or over ssh. Then press Ctrl+Alt+F3:

```
# systemctl start vtium@tty3.service
```

**Note:** `vtium console` starts and stops nothing. Starting or stopping a console on a VT ends every session there, including the one you type in. Each verb prints the `systemctl` command that makes its change now.

To give vtium every free VT again, as the install does:

```
# vtium console enable --direct --all
```

`enable --all` links the template link, `/etc/systemd/system/autovt@.service`, to vtium. logind follows the template link on every VT without a link of its own. `enable --all` also runs `enable N` on each free VT from 1 to 6. So systemd starts vtium on each of those VTs at boot, in place of the getty that `enable` disables. vtium then holds each of them open from boot. `enable --all` leaves these VTs alone, and names each one:

- a VT that another program uses now, or will claim at the next boot, as the next section describes.
- a VT where `kmscon` is enabled, linked, or running.
- a VT that the development install holds. See [The development VT](vt-review.md#installing-vtium-on-a-vt) for details.
- a VT whose own link, `/etc/systemd/system/autovt@ttyN.service`, names another console.
- a VT above 6. logind starts no console there by itself. `enable N` gives vtium such a VT at boot.

VT 6 is logind's reserved VT. logind starts a console there at every switch, even while the VT is in use. `enable 6` and `enable --all` refuse while someone is logged in on VT 6. They go ahead when that login is inside a vtium running there. A getty that waits at its login prompt on VT 6 holds no session, so both commands go ahead. The next switch to VT 6 then stops that getty. `enable --all` also refuses when VT 6 has no link of its own and `kmscon` or the development install is enabled or running there. It refuses rather than passing VT 6 by.

### VTs that other programs hold

`vtium console` follows common convention. On a machine with a display manager, VT 1 stays the display manager's and its getty's, and vtium takes other VTs. systemd's preset and the display managers' own units leave VT 1 that way.

vtium never takes a VT that your system is set up to give to another program. That program is either a unit enabled at boot that claims the VT, or an enabled display manager. A program running on a VT now holds that VT as well. But what runs now never frees a VT. `enable N` refuses a VT that another program holds, and `enable --all` passes it by. Both say what holds the VT, and how vtium knows. These hold a VT:

- An enabled display manager holds VT 1, whatever VT the display manager uses now. vtium knows that a display manager is enabled in one of two ways. Either `/etc/systemd/system/display-manager.service` links to it, or a unit that starts at boot runs a display manager that vtium knows by name. An example of the second is ly's `ly@tty2`, which ly enables without that link.
- A unit that starts at boot holds a VT when it names the VT's getty in `Conflicts=`. For example, Arch's `sddm.service` names `getty@tty1.service`.
- A graphical session or a greeter on the VT now holds it. vtium asks logind for these.
- A program whose controlling terminal is the VT now holds it, with or without a logind session. An example is the X server that SDDM's X11 greeter runs on VT 2. The controlling terminal is the terminal that the kernel ties a program's session to. vtium reads it from `/proc`. A getty on the VT and a login on the VT do not count.
- Another console running on the VT now holds it, such as `fbterm@tty4.service`.
- A unit that starts at boot and runs on the VT holds it, such as `cage@tty3.service` or a unit with `TTYPath=/dev/tty9`.
- Your display manager's own configuration holds the VT that it starts its greeter on. vtium reads LightDM's `minimum-vt`, which is 7 unless you set it, greetd's `vt`, and SDDM's `MinimumVT`.

Nothing frees a VT. For example, a display manager's greeter may run on VT 2. VT 1 still stays the display manager's. `vtium console status` gives the reasons under its table.

If vtium fails on a VT, its fallback starts that VT's getty again, so that you have a login. The fallback never does this when the getty's start would stop another program. For example, starting `getty@tty1` would stop a display manager whose unit conflicts with it. Then the VT is left without a console, and `journalctl -b -u vtium-getty@tty1.service` says why. To find out why vtium failed, log in on another VT or over ssh. See [A VT that shows a getty](troubleshooting.md#a-vt-that-shows-a-getty) for details.

On a machine where another terminal, such as `kmscon`, holds the template link or runs on your VTs, read [Moving from another DRM/KMS VT](#moving-from-another-drmkms-vt) before you run `enable --all`.

Before `enable` changes anything, it records what each VT had, in `/var/lib/vtium/console/`. `disable` puts back exactly that:

```
# vtium console disable 3
# vtium console disable --all
```

`disable 3` takes vtium off VT 3's boot list. It removes VT 3's own link to vtium, and gives VT 3 back the getty it had. `disable --all` does that on every VT. After `enable --all`, though, logind still starts vtium on VT 3 through the template link. It does so whenever you switch to VT 3 while nothing runs there, and `disable 3` tells you so. Only `disable --all` points the template link back where it pointed before.

[crates/vtium/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#setting-it-up) gives the full rules. On a machine that was not booted with systemd, `vtium console` changes nothing, and says so. There you replace a VT's getty entry with one that runs `vtium --vt=ttyN --no-switchvt`, in the way your init system runs the getty. [Giving vtium a VT without systemd](systemd.md#giving-vtium-a-vt-without-systemd) shows how for sysvinit, OpenRC, and runit.

## Moving from another DRM/KMS VT

Other terminals also render on a VT through [DRM/KMS](https://en.wikipedia.org/wiki/Direct_Rendering_Manager), the modern graphics interface. `kmscon` is one of them. Such a terminal starts on a VT in one of two ways:

- from a unit enabled at boot for that VT;
- from the template link `/etc/systemd/system/autovt@.service`, which logind follows when you switch to a VT that nothing runs on.

`vtium console` passes by every VT where another terminal is enabled at boot or still runs. See [VTs that other programs hold](#vts-that-other-programs-hold) for details. So take those VTs back from the other terminal first. Do this from a desktop terminal or over ssh, after you log out of each VT where the other terminal runs. With `kmscon`, whose unit is `kmsconvt@.service`:

```
# systemctl disable kmsconvt@.service
$ systemctl list-units 'kmsconvt@*'
# systemctl stop kmsconvt@tty2.service
# vtium console enable --direct --all
```

Run the `stop` line for each VT that `list-units` shows. For another terminal, put its own unit's name in place of `kmsconvt@`. `systemctl disable` stops nothing. Logging out of a VT leaves the terminal running there.

`vtium console enable --direct --all` refuses while the template link names `kmscon`'s unit. So on such a machine, the package's install gives vtium no VT until you do this. The template link may name another terminal's unit instead. Then `enable --all` records the link before it points the link at vtium, and `disable --all` points it back.

You can remove `kmscon` afterwards with `pacman -R kmscon`, if you want to. vtium needs nothing from it. vtium never reads `/etc/kmscon/kmscon.conf`. But vtium takes the keys that `kmscon` used. So you can copy that file to `/etc/vtium/vtium.conf` by hand, and check it with `vtium check-config`. [config.md](config.md) lists every key that vtium reads. [crates/vtium/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#sharing-vts-with-kmscon) says when to keep `kmscon-terminfo`.

## Upgrading and removing

An upgrade restarts no running vtium, because a restart ends every session on its VT. Each VT runs the new build from its next start. To restart vtium on VT 3 now, log out there. Then run this from another VT or over ssh:

```
# systemctl restart vtium@tty3.service
```

When you remove the package, its removal script first runs `vtium console disable --all`. That gives every VT back what it had. The script stops nothing, and [the note above](#giving-vtium-a-vt) says why. So stop vtium yourself on each VT where it runs. Do this from another VT or over ssh, before or after the removal:

```
$ systemctl list-units 'vtium@*'
# systemctl stop vtium@tty3.service
# pacman -R vtium
```

`disable --all` puts back what each VT had, and then forgets its record. So `/var/lib/vtium/console/` is left empty, and you can remove it. You may replace one console package with another, such as `vtium-minspec` in place of `vtium`. Then the old package gives the VTs back, and the new one takes the free VTs again. The [vtium-arch README](https://github.com/vtium/vtium-arch#removing-vtium) says what to do when you removed the package first.

## Sharing VTs with kmscon

`kmscon` and vtium can each keep VTs of their own on one machine. A VT can have its own link, `autovt@ttyN.service`, and that link wins over the template link. To give `kmscon` a VT that vtium holds, VT 3 in this example, first take VT 3 back from vtium. Then link VT 3 to `kmscon`:

```
# vtium console disable 3
$ vtium console status
# systemctl disable getty@tty3.service
# ln -s /usr/lib/systemd/system/kmsconvt@.service /etc/systemd/system/autovt@tty3.service
# systemctl daemon-reload
```

`disable 3` gives VT 3 back the getty it had. See [Giving vtium a VT](#giving-vtium-a-vt) for details. Run the `systemctl disable getty@tty3.service` line only where `status` then shows a getty at boot on VT 3. A getty that systemd starts at boot holds the VT, and logind then starts nothing there.

The new link does two things. It keeps logind from starting vtium on VT 3 through the template link after `enable --all`. It also gives VT 3 to `kmscon`, whatever the template link names. logind starts `kmscon` the next time you switch to VT 3 while nothing runs there. A vtium that still runs on VT 3 keeps the VT until you stop it. [crates/vtium/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#sharing-vts-with-kmscon) gives the way back, and the other direction.
