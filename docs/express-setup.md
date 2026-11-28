# Set up vtium on an Arch desktop

This guide sets up vtium on an Arch Linux desktop in about ten minutes. vtium runs on a Linux virtual terminal (VT), one of the text screens you reach with Ctrl+Alt and a function key. It takes the place of getty, the program that shows the login prompt there. At the end, vtium runs on the VTs from 2 to 6 that your desktop leaves free. It renders through the GPU in Cullet Mono, and you can switch to JetBrains Mono, Monoid or another font at any time.

The guide assumes a common desktop: Arch Linux with systemd, a GPU, and a desktop such as KDE Plasma behind a display manager such as SDDM. The kernel's framebuffer console, fbcon, stays as it is. vtium renders over fbcon on its own VTs and leaves every other VT to fbcon. [Install vtium and give it VTs](install.md), [Configure vtium](configure.md) and [Choose a font](fonts.md) cover each step in depth.

In the commands below, `#` marks a command that you run as root, for example with sudo. `$` marks a command that you run as your own user.

## Before you start

Run the commands on this page in a terminal on your desktop, such as Konsole, while you are logged in there. Your desktop session then holds its VT, and vtium leaves that VT alone.

Find the VT your desktop runs on. The TTY column of this command names it, such as `tty2`:

```
$ loginctl list-sessions
```

From any VT, Ctrl+Alt and that VT's function key bring you back to the desktop, for example Ctrl+Alt+F2 for `tty2`. Ctrl+Alt+F1 shows SDDM's login screen. If vtium stops answering keys, the switch fails too. Then log in over ssh from another machine, if you have one, and give the VT back as [Restore the machine](#restore-the-machine) shows.

vtium records what each VT ran before it changes the VT, and puts that back when you give the VT back. The two files this guide writes, `/etc/vtium/vtium.conf` and `/etc/vtium/batch.conf`, are new. If you tried vtium before and `/etc/vtium` exists, keep a copy of it:

```
# cp -a /etc/vtium /root/vtium-before
```

## Install vtium and its fonts

The AUR (the Arch User Repository) carries vtium's packages. Build and install `vtium-terminfo` first, then `vtium`, then `vtium-fonts`. makepkg checks each one against the maintainer's signing key, and each recipe carries a copy of that key:

```
$ git clone https://aur.archlinux.org/vtium-terminfo.git
$ cd vtium-terminfo
$ gpg --import keys/pgp/33F9B54FD2A4B6A4110749B3BA464FC9847A832A.asc
$ makepkg -si
$ cd ..
$ git clone https://aur.archlinux.org/vtium.git
$ cd vtium
$ makepkg -si
$ cd ..
$ git clone https://aur.archlinux.org/vtium-fonts.git
$ cd vtium-fonts
$ makepkg -si
$ cd ..
# pacman -S ttf-nerd-fonts-symbols-mono
```

An AUR helper such as paru or yay builds the three AUR packages in one step instead. The `vtium` package brings Cullet Mono and the font sets `cullet-mono` and `jetbrains-mono`. A font set, or frit, lists a font and the fonts that fill in the characters it lacks. `vtium-fonts` brings the rest of vtium's catalog of font sets, `monoid` among them. It also installs JetBrains Mono, Monoid and Noto Color Emoji from Arch's repositories. `ttf-nerd-fonts-symbols-mono` adds the icons that shell prompts use.

The install gives vtium every free VT from that VT's next start, and starts vtium nowhere. VT 1 stays SDDM's, because SDDM's unit claims VT 1. The VT of your running desktop session stays the desktop's. systemd starts vtium at boot on each other VT from 2 to 6. The install message names each VT that vtium leaves alone, and why. To see what each VT runs at any time:

```
$ vtium console status
```

vtium does not start at boot on your desktop's VT, so SDDM finds that VT free and puts your next desktop session there. If you switch to that VT before you log in to the desktop, vtium starts there instead, and the desktop moves to VT 7 or above. [VTs that other programs hold](install.md#vts-that-other-programs-hold) gives the full rules.

## Write the configuration

This file sets the GPU path, the font set and the size, and leaves every other key at its default. Paste the whole block into your terminal. sudo asks for your password, then writes the file as root:

```
$ sudo tee /etc/vtium/vtium.conf > /dev/null <<'EOF'
# vtium.conf from "Set up vtium on an Arch desktop".
# Every key that this file does not name keeps its default.

# Render through the GPU. Write no-hwaccel to render on the CPU.
hwaccel

# The font set: Cullet Mono, then Nerd Fonts icons, color emoji,
# and GNU Unifont for every other character.
# For another font, change this line to font-name=jetbrains-mono
# or font-name=monoid.
font-name=cullet-mono

# The font size in points. Ctrl+= and Ctrl+- zoom during a session.
font-size=12

# Let vtium config use-frit and vtium-config's tryouts switch the font.
allow-font-ops
EOF
```

[Configuration keys](config.md) describes every key.

## Activate the fonts

vtium renders only the active fonts, which `/etc/vtium/batch.conf` lists. A fresh install activates vtium's own fonts, Cullet Mono and GNU Unifont among them. This command activates JetBrains Mono, Monoid, the emoji and the icons too:

```
$ vtium config activate /usr/share/fonts/TTF/JetBrainsMono-*.ttf /usr/share/fonts/TTF/Monoid-*.ttf /usr/share/fonts/noto/NotoColorEmoji.ttf /usr/share/fonts/TTF/SymbolsNerdFontMono-Regular.ttf --install
```

`vtium config` runs vtium-config, vtium's setup tool. It keeps your changes in a draft, a copy of vtium.conf in your home directory, which it makes from the file you just wrote. `--install` then installs the draft through sudo, and the install writes `/etc/vtium/batch.conf`. Then check the installed configuration:

```
$ vtium check-config
```

`vtium check-config` reads the configuration as vtium does when it starts. It prints a few notes about the defaults, names each problem it finds, and exits 0 when it finds none.

You can make the same change in the draft alone instead. Run `vtium config activate` without `--install`, then `vtium config edit` to paste the file into the draft, then `vtium config install`. [Configure vtium](configure.md#change-it-with-vtium-config) describes the draft.

## Start vtium and log in

vtium reads vtium.conf when it starts on a VT. To start vtium on VT 3 now, run the command below. Choose another VT from 2 to 6 if VT 3 is your desktop's. Starting vtium ends any text login on that VT.

```
# systemctl start vtium@tty3.service
```

Press Ctrl+Alt+F3, and log in at vtium's login prompt. From the next boot, vtium starts by itself on each of its VTs.

## Try other fonts

In a session in vtium, these commands switch the console to another font set:

```
$ vtium config use-frit jetbrains-mono
$ vtium config use-frit monoid
$ vtium config use-frit cullet-mono
```

Ctrl+= zooms in, Ctrl+- zooms out, and Ctrl with the mouse wheel zooms too. A switch and a zoom last until you log out. vtium then returns to the font and size that vtium.conf sets.

To compare font sets side by side, open vtium-config's full-screen view:

```
$ vtium config tui
```

Press `3` for the Looks tab, highlight a font set, and press `t`. The console shows that font set for 10 seconds. Press `k` to keep it until you quit the tool, or any other key to put the old font back. Press `1` for the Fonts tab, where `t` tries one font file the same way.

To keep a font set, set it as `font-name` and install the draft:

```
$ vtium config set font-name jetbrains-mono --install
```

vtium reads `font-name` and `font-size` again when a session ends, so the next login on each VT uses the new font. Another set from the catalog, such as `fira-code`, needs its font files activated first, as in [Activate the fonts](#activate-the-fonts). For a font of your own, [Choosing a font with vtium config](fonts.md#choosing-a-font-with-vtium-config) gives the steps.

## Restore the machine

### Give the VTs back

These commands give every VT back what it ran before, and keep vtium installed:

```
# vtium console disable --all
$ systemctl list-units 'vtium@*'
# systemctl start getty@tty3.service
```

`vtium console disable --all` takes effect at each VT's next start. Starting a VT's getty stops vtium there at once, and ends every session on that VT. Run the `start` line for each VT that `list-units` shows.

### Remove vtium

Stop vtium on each VT where it runs, then remove the packages:

```
$ systemctl list-units 'vtium@*'
# systemctl stop vtium@tty3.service
# pacman -Rs vtium vtium-fonts vtium-terminfo ttf-nerd-fonts-symbols-mono
```

The removal runs `vtium console disable --all` first, so every VT gets back what it ran before. `-s` also removes the fonts that `vtium-fonts` pulled in, unless you installed them yourself or another package needs them. Then remove the files that vtium-config wrote, and your drafts:

```
# rm -r /etc/vtium /var/lib/vtium
$ rm -r ~/.config/vtium ~/.local/state/vtium
```

If you kept a copy of `/etc/vtium` before you started, put it back with `cp -a /root/vtium-before /etc/vtium` instead of removing the folder. SDDM, your desktop and fbcon need no restore, because vtium does not change them.

### Boot once without vtium

Add `vtium.enable=0` to the kernel command line in your boot loader. vtium then exits as soon as it starts, and a getty's login prompt starts on each of its VTs. [Find out what went wrong](troubleshooting.md) covers the other ways out of a VT where vtium misbehaves.
