# Install vtium and give it VTs

This guide installs vtium, says which VTs the package gives it, and shows how you choose other VTs. A VT, or virtual terminal, is one of the full-screen text screens the Linux kernel keeps beside a desktop, and vtium runs on a VT in the place of the getty, the program that prints the login prompt there. If you have never run vtium, [Try vtium on one VT](tutorial-try-vtium.md) walks through a first install on one VT and back again.

In the commands below, `#` marks a command you run as root, for example with sudo, and `$` a command you run as your own user.

## Arch Linux

The AUR (the Arch User Repository) carries vtium's packages for Arch. Each package is built from a recipe, a `PKGBUILD`, which is the build script that makepkg reads. The repository `vtium/vtium-arch` keeps nine recipes, one for each package. The AUR carries five of them. The console packages conflict with each other, so install one:

| Package | What it builds | Built from | On the AUR |
|---|---|---|---|
| `vtium` | the console for an ordinary desktop or laptop | a release: the tag the maintainer signs, the latest on [the releases page](https://github.com/vtium/vtium/releases) | yes |
| `vtium-git` | the same console | the tip of vtium's `main` branch | yes |
| `vtium-minspec` | vtium's smallest console, with no pictures and no GPU path | the same release tag | no: build it from the `vtium` recipe ([below](#build-the-minspec-console)) |
| `vtium-minspec-git` | the same console | the tip of `main` | no: build it from the `vtium-git` recipe |
| `vtium-minspec-gpu` | the smallest console with the GPU path | the same release tag | no: build it from the `vtium` recipe |
| `vtium-minspec-gpu-git` | the same console | the tip of `main` | no: build it from the `vtium-git` recipe |
| `vtium-terminfo` | `xterm-vtium`, the terminfo entry that sessions in vtium name in `TERM` | the same release tag | yes |
| `vtium-terminfo-git` | the same entry | the tip of `main` | yes |
| `vtium-fonts` | a curated set of fonts chosen to go with vtium's frits, through the distribution's own font packages | the recipe repository alone | from the next publish |

Each console package depends on a terminfo package: a console built from the release tag on `vtium-terminfo`, and one built from `main` on `vtium-terminfo-git`. Today the AUR builds `vtium-terminfo` from the `vtium` recipe and `vtium-terminfo-git` from the `vtium-git` recipe. From the next publish, each terminfo package has a recipe of its own on the AUR.

A recipe builds the console with the features that `VTIUM_FEATURES` names in makepkg's environment. The `images` feature draws sixel and kitty graphics pictures, the `logind` feature lets the suspend key ask logind, systemd's login manager, to suspend the machine, the `gpu` feature builds in the path that draws through the GPU, and the `bidi` feature lays out right-to-left text such as Arabic and Hebrew. When `VTIUM_FEATURES` is unset, the `vtium` and `vtium-git` recipes build `images logind gpu bidi`, for an ordinary desktop or laptop. To build another set of features from a checkout, stage it with `make install` ([From a checkout](#from-a-checkout)).

From v0.1.0-alpha.6 on, the `vtium` and `vtium-git` recipes build with `HWACCEL=on DISPLAY_BUFFERS=3`, which compiles in `hwaccel` on and three buffers for each display as the defaults ([config.md](config.md)). The console therefore draws on the GPU path, and draws a head on the CPU when the head's GPU set-up fails. A line `no-hwaccel` in `/etc/vtium/vtium.conf` turns the GPU path off. v0.1.0-alpha.5 built with `HWACCEL=off`, so its console drew on the CPU until a line `hwaccel` in `/etc/vtium/vtium.conf` turned the GPU path on. `vtium` and `vtium-git` depend on `libglvnd` and `mesa`, which hold the two libraries the GPU path loads, `libEGL.so.1` and `libgbm.so.1`.

No package ships a font. The console draws with its built-in fonts, unifont and unscii-8. `vtium` and `vtium-git` install three frits (font set files) that name fonts the distribution packages, `desktop`, `minspec-16` and `minspec-8`. A frit leaves out a font the machine lacks, so a frit with none of its fonts installed draws the built-in fonts. On Arch the frits' fonts are in `ttf-jetbrains-mono`, `ttf-nerd-fonts-symbols-mono` and `noto-fonts-emoji`.

makepkg refuses to build the `vtium` recipe until your keyring holds the maintainer's signing key, `33F9B54FD2A4B6A4110749B3BA464FC9847A832A`. The recipe carries a copy of the key, so import it once from there, then build and install. The example builds pictures, the power keys, the GPU path and right-to-left text, the features the recipe builds by default:

```
$ git clone https://aur.archlinux.org/vtium.git
$ cd vtium
$ gpg --import keys/pgp/33F9B54FD2A4B6A4110749B3BA464FC9847A832A.asc
$ VTIUM_FEATURES='images logind gpu bidi' makepkg -si
```

`gpg --recv-keys 33F9B54FD2A4B6A4110749B3BA464FC9847A832A` fetches the same key from a keyserver instead. `vtium-git` checks no signature, because GitHub signs the merge commits on `main` with its own key.

The packages install these files:

| File | What it is |
|---|---|
| `/usr/bin/vtium` | the console, with its subcommands `vtium console`, `vtium check-config`, `vtium config`, `vtium blank` and `vtium wake` |
| `/usr/bin/vtium-config` | the configuration tool, which `vtium config` runs: it keeps your changes in a draft and installs the draft as `/etc/vtium/vtium.conf` ([Configure vtium](configure.md)), `vtium config tui` opens its full-screen view, which also tries each font on the live console, and `vtium config use-frit` switches the font; it is also the glyph helper, which draws a font's glyphs for the console |
| `/usr/lib/systemd/system/vtium@.service` | the unit that runs vtium on one VT in place of its getty |
| `/usr/lib/systemd/system/vtium-getty@.service` | the fallback that starts a getty on the VT when vtium fails |
| `/usr/bin/vtium-image` | the image helper, which reads a picture a program names by its file, in a console built with `images` |
| `/usr/lib/sysusers.d/vtium-image.conf` | the image helper's account, in a console built with `images` |
| `/usr/bin/vtium-gallery` | the picture gallery tool, which shows PNG pictures as a grid of thumbnails, in a console built with `images` |
| `/etc/vtium/vtium.conf.example` | the example configuration, which lists every key at its default, commented out, and then the heads and rules vtium runs with no configuration |
| `/etc/vtium/fonts/` | the frits `desktop`, `minspec-16` and `minspec-8`; each is ready to choose with `vtium config` or vtium.conf's `font-name`, and a frit that nothing names changes nothing |
| `/usr/share/doc/vtium/examples/` | an example udev rule and hwdb fragment, which are documentation and never active |
| `/usr/share/terminfo/x/xterm-vtium`, `/usr/share/terminfo/v/vtium` | the terminfo entry, under its name and its alias `vtium`, from the terminfo package |
| `/usr/share/licenses/<package>/LICENSE` | the license, in each package |
| `/usr/share/licenses/<package>/COPYING` | vtium's authors and the notices for the third-party code and data vtium builds in, in each package except `vtium-fonts`, which carries only its own `LICENSE` |
| `/usr/share/licenses/<package>/OFL-1.1.txt`, `rgb-license.txt` | the two license texts that `COPYING` points to, the SIL Open Font License of the Unifont font and the license of X.Org's color names, in each console package |

When you first install a console package, it gives vtium every free VT and starts nothing. [What the install does to your VTs](#what-the-install-does-to-your-vts) explains what that means.

### Build the minspec console

The minspec console is vtium's smallest build. It suits a small machine such as a single-board computer, a server's console or a kiosk. The AUR carries no minspec package, so you build the minspec console from the `vtium` recipe, or from the `vtium-git` recipe for the tip of `main`. Set `VTIUM_FEATURES` but leave it empty to build the console with no features, which means no pictures, no logind and no GPU path:

```
$ cd vtium
$ VTIUM_FEATURES= makepkg -si
```

`VTIUM_FEATURES=gpu makepkg -si` builds the minspec console with the GPU path alone. `vtium --version` names the features a console was built with, such as `features none` or `features gpu`.

`VTIUM_FEATURES` changes only the features. The package you build keeps everything else the recipe sets: its dependencies on `libglvnd` and `mesa`, its frits, and its defaults for `hwaccel` and `display-buffers`. A line in `/etc/vtium/vtium.conf` changes either default, such as `display-buffers=2` for two buffers for each display. See [config.md](config.md) for the keys.

The repository `vtium/vtium-arch` also keeps four minspec recipes, `vtium-minspec` and `vtium-minspec-gpu` with their `-git` twins. Its CI builds them to check that the minspec console still builds.

### What the next publish adds

`vtium-fonts` reaches the AUR with the next publish. It is a curated set of fonts chosen to go with the frits (font set files) that vtium ships, and it grows as new frits are added. It installs no font file itself. Instead it depends on the distribution's packages for each font the frits name: on Arch `ttf-jetbrains-mono`, `ttf-nerd-fonts-symbols-mono`, `noto-fonts-emoji` and `terminus-font`. The console packages list it as an optional dependency, so you install it only if you want those fonts.

The `vtium` and `vtium-git` recipes in `vtium/vtium-arch` also ship a fourth frit, `terminus`, for the Terminus console font, which the packages the AUR carries today do not have yet.

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

A package build that runs with no network, as Debian's does, takes the crates vtium builds with from the release. Each GitHub release from v0.1.0-alpha.6 on carries `vtium-VERSION-vendor.tar.zst`, where VERSION is the release's version without its leading v, and the file's SHA-256 in `vtium-VERSION-vendor.tar.zst.sha256`. The archive holds the source of every outside crate in `Cargo.lock`, for every architecture, and a `.cargo/config.toml` that tells Cargo to read them from `vendor/`. Unpack it at the top of the release's source tree, beside `Cargo.toml`, with `tar -xf vtium-VERSION-vendor.tar.zst -C DIR`; it adds `vendor/` and `.cargo/config.toml` and replaces none of vtium's files. `make install` and `cargo build --offline --locked` then build with no network. A CI workflow packs the archive when a release is published and builds vtium offline from it before it attaches it to the release. In a checkout, `make vendor-archive` packs the same archive for the commit you have checked out, and `make vendor-archive-check` also builds vtium offline from it.

With both `libseat` and `logind` in `FEATURES`, `make install` also stages a second unit, `vtium-seat@.service`, and the PAM file `/etc/pam.d/vtium`. They run vtium in seat mode, which [Direct mode and seat mode](#direct-mode-and-seat-mode) describes. See [systemd.md](systemd.md) for details.

`make install-tree` stages the same programs under `target/install-tree`: the console, `vtium-config`, the VT unit and its getty fallback, the terminfo entry, the example configuration and the frits. It also needs `tic`, which compiles the terminfo entry. Tier 3, vtium's test harness on a virtual machine, installs that tree in its guest.

## What the install does to your VTs

When you first install a console package, its install script runs `vtium console enable --direct --all`. That gives vtium every free VT: every VT that no display manager, greeter, graphical session or `kmscon` holds. The script starts nothing, so each VT changes at its next start:

- On a machine with no display manager, vtium runs on VT 1 from the next boot, in place of its getty. VT 1 is where the kernel shows the boot messages, so vtium takes over the screen where the boot appeared.
- On a machine with a display manager, VT 1 stays the display manager's, and vtium takes only the other free VTs.
- On each other free VT, logind starts vtium the first time you switch to it, as it starts a getty there today. vtium does not run on a VT that nobody visits.

The install message shows what `vtium console` did, and names each VT it left alone and why. An upgrade changes no VT, so if you gave the VTs back, they stay back. [Giving vtium a VT](#giving-vtium-a-vt) shows how you change the choice.

`vtium console` refuses to change anything while `kmscon` holds the template link `/etc/systemd/system/autovt@.service`, while `loginctl` cannot say which sessions run, and on a machine not booted with systemd, such as a chroot that an installer prepares. The install still succeeds. Its message then says that vtium took no VT, gives the reason, and names the command to run once the reason is gone:

```
# vtium console enable --direct --all
```

On a machine where `kmscon` holds the template link, read [Moving from another DRM/KMS VT](#moving-from-another-drmkms-vt) first.

### Direct mode and seat mode

vtium can run on a VT in two ways, and the install turns on the first:

- In direct mode, vtium opens the screen and the keyboards itself. Each login at vtium's prompt gets an active logind session on seat0, as a login at a getty does. So sound, removable disks and the actions that polkit allows the active session work as usual. The unit is `vtium@.service`.
- In seat mode, vtium takes the screen, the keyboards and VT switching from logind, the service that manages logins and seats, through libseat. Each login gets a logind session with no seat. polkit treats that session as inactive, and udev gives it no devices, such as sound cards. The unit is `vtium-seat@.service`, which a console built with `libseat` and `logind` ships, as the `vtium` package is built.

A VT runs one mode at a time. To ask which mode fits your machine, run `vtium console enable` with no mode. It changes nothing: it says what it finds, which mode it recommends and why, and the command to run. To move every VT vtium has to seat mode, and back:

```
# vtium console enable --seat --all
# vtium console enable --direct --all
```

`vtium console enable --seat` refuses, and changes nothing, on a console built without `libseat` and `logind`, and on a machine where logind manages no seat0. See [the libseat issue](https://github.com/vtium/vtium/issues/34) for the work on seat mode.

### When vtium fails on a VT

Each VT that vtium takes keeps its own getty as the fallback. If vtium fails on a VT, the VT's fallback starts that VT's getty, so you have a login prompt there. [VTs that other programs hold](#vts-that-other-programs-hold) describes the one case where the fallback starts nothing.

vtium also steps aside for systemd's rescue and emergency modes, as a getty does. When systemd enters either mode, it stops vtium on every VT first, and the rescue shell asks for root's password on the VT that is shown.

### Turning vtium off for one boot

To start a machine once without vtium, add `vtium.enable=0` to the kernel command line in your boot loader. vtium then exits as soon as it starts, before it touches the screen, and each VT's getty fallback gives you a login prompt. The key works with or without systemd. Without systemd, what runs on the VT after vtium exits is your init system's choice. With systemd, `systemd.unit=rescue.target` on the kernel command line boots into rescue mode instead, and vtium steps aside for it.

**Note:** Masking vtium's unit, as with `systemd.mask=vtium@tty2.service`, does not turn vtium off on every VT. logind starts vtium through the template link `autovt@.service`, and a mask does not reach a unit that systemd loads through a link. After the install, VT 1 also has no getty enabled, so a mask of `vtium@tty1.service` leaves VT 1 with no console.

### The kernel's messages

The kernel draws its own text on a VT through fbcon, the framebuffer console. vtium draws over fbcon on the VTs it holds, and leaves fbcon to draw every other VT, so a getty and the rescue shell still show their text. Because fbcon draws nothing on vtium's VT, vtium shows the kernel's messages itself. It shows each message from the kernel whose level is warning or more severe, on the boot display, over the console. It never writes them into a session's screen. `quiet` or `loglevel=3` on the kernel command line lets fewer messages through, and vtium then shows fewer too.

Plymouth, the boot splash program, shows the kernel's messages itself while it runs. The vtium.conf key `kernel-messages` decides who shows them then:

| Value | What vtium does |
|---|---|
| `after-plymouth` | the default: vtium leaves the messages to Plymouth while Plymouth runs, and shows them itself once Plymouth has quit. On a machine without Plymouth, vtium shows them from its start. |
| `always` | vtium shows the messages whether Plymouth runs or not. |

At a kernel panic, the kernel stops every program, vtium included. What the screen then shows is the kernel's own panic screen, on a kernel and a display driver that support one. Starting vtium earlier, from the initramfs, so that it shows the boot from its first second, is [the boot issue](https://github.com/vtium/vtium/issues/266).

## Giving vtium a VT

The package's install has already run `vtium console enable --direct --all`. `vtium console` gives a VT to vtium, or every VT, and takes them back, so you can change that choice at any time. It changes which console systemd starts on a VT at boot, and which console logind starts when you switch to a VT where nothing runs. Each call of `enable` or `disable` takes one VT by its number, or `--all`. `enable` and `disable` need root, and `status` runs as any user.

See first what each VT runs now:

```
$ vtium console status
```

[Reading vtium console status](troubleshooting.md#reading-vtium-console-status) explains its columns.

To give vtium one VT alone, VT 3 here, first take every VT back with `disable --all` (below), then:

```
# vtium console enable --direct 3
```

vtium is then VT 3's console from its next start: systemd starts `vtium@tty3` there at boot, in the place of the getty it disables. `enable` prints the command that starts vtium now. Log out of VT 3, run that command from another VT or over ssh, and press Ctrl+Alt+F3:

```
# systemctl start vtium@tty3.service
```

**Note:** `vtium console` starts and stops nothing, because starting or stopping a console on a VT ends every session there, the one you type in included. Each verb prints the `systemctl` command that makes its change now.

To give vtium every free VT again, as the install does:

```
# vtium console enable --direct --all
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
# vtium console enable --direct --all
```

Run the `stop` line for each VT that `list-units` shows. For another terminal, put its own unit's name in place of `kmsconvt@`. `systemctl disable` stops nothing, and logging out of a VT leaves the terminal running there. `vtium console enable --direct --all` refuses while the template link names `kmscon`'s unit, so on such a machine the package's install gives vtium no VT until you do this. When the template link names another terminal's unit, `enable --all` records the link before it points it at vtium, and `disable --all` points it back.

Removing `kmscon` afterwards is up to you, with `pacman -R kmscon`; vtium needs nothing from it. vtium never reads `/etc/kmscon/kmscon.conf`. vtium takes the keys `kmscon` used, so you can copy that file to `/etc/vtium/vtium.conf` by hand and check it with `vtium check-config`; [config.md](config.md) lists every key vtium reads. [crates/vtium/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#sharing-vts-with-kmscon) says when to keep `kmscon-terminfo`.

## Upgrading and removing

An upgrade restarts no running vtium, since a restart ends every session on its VT. Each VT runs the new build from its next start. To restart VT 3 now, log out there, then from another VT or over ssh:

```
# systemctl restart vtium@tty3.service
```

When you remove the package, its removal script first runs `vtium console disable --all`, which gives every VT back what it had. It stops nothing ([the note above](#giving-vtium-a-vt) says why), so stop vtium where it runs, from another VT or over ssh, before or after the removal:

```
$ systemctl list-units 'vtium@*'
# systemctl stop vtium@tty3.service
# pacman -R vtium
```

`disable --all` puts back what each VT had and then forgets its record, so `/var/lib/vtium/console/` is left empty, and you can remove it. When you replace one console package with another, such as `vtium-minspec` in place of `vtium`, the old package gives the VTs back and the new one takes the free VTs again. The [vtium-arch README](https://github.com/vtium/vtium-arch#removing-vtium) says what to do when you removed the package first.

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
