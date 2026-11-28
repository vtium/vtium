# Set up vtium on an Arch desktop

This guide sets up vtium on an Arch Linux desktop in a few minutes. vtium runs on a Linux virtual terminal (VT), one of the text screens you reach with Ctrl+Alt and a function key. It takes the place of getty, the program that shows the login prompt there. At the end, vtium runs on the free VTs from 2 to 6 and renders through the GPU in Cullet Mono. You can switch it to JetBrains Mono or Monoid at any time.

The guide assumes Arch Linux with systemd, a GPU, and a desktop such as KDE Plasma behind a display manager such as SDDM. [Install vtium and give it VTs](install.md), [Configure vtium](configure.md) and [Choose a font](fonts.md) cover each step in depth.

## Before you start

Run the commands on this page in a terminal on your desktop, such as Konsole. Your desktop session then holds its VT, and vtium leaves that VT alone.

Ctrl+Alt+F1 shows SDDM's login screen, and the function key of your desktop's VT brings you back to the desktop. The TTY column of `loginctl list-sessions` names that VT, such as `tty2` for Ctrl+Alt+F2.

If you tried vtium before, keep a copy of its configuration:

```
$ sudo cp -a /etc/vtium /root/vtium-before
```

## Install vtium and its fonts

An AUR helper such as paru installs vtium, its terminfo entry, its catalog of font sets, and the icons that shell prompts use. yay works the same way. When the helper asks to import the maintainer's signing key, answer yes:

```
$ paru -S vtium vtium-fonts ttf-nerd-fonts-symbols-mono
```

Without a helper, build each AUR package with makepkg, which checks it against the key that each recipe carries:

```
$ for p in vtium-terminfo vtium vtium-fonts; do git clone https://aur.archlinux.org/$p.git; (cd $p && gpg --import keys/pgp/*.asc && makepkg -si); done
$ sudo pacman -S ttf-nerd-fonts-symbols-mono
```

The `vtium` package brings Cullet Mono. `vtium-fonts` brings the font sets `monoid` and many more, with JetBrains Mono, Monoid and Noto Color Emoji from Arch's repositories. A font set, or frit, lists a font and the fonts that fill in the characters it lacks.

The install gives vtium each VT from 2 to 6 that no other program holds, and starts vtium nowhere yet. VT 1 stays SDDM's, and your desktop's VT stays the desktop's. [VTs that other programs hold](install.md#vts-that-other-programs-hold) gives the full rules.

## Choose the fonts

vtium renders only the active fonts, which `/etc/vtium/batch.conf` lists. These two commands activate JetBrains Mono, Monoid, the emoji and the icons, choose Cullet Mono, and install the result:

```
$ vtium config activate /usr/share/fonts/TTF/JetBrainsMono-*.ttf /usr/share/fonts/TTF/Monoid-*.ttf /usr/share/fonts/noto/NotoColorEmoji.ttf /usr/share/fonts/TTF/SymbolsNerdFontMono-Regular.ttf
$ vtium config set font-name cullet-mono --install
```

`vtium config` runs vtium-config, vtium's setup tool, which keeps your changes in a draft in your home directory. `--install` checks the draft, then asks sudo for your password once and writes `/etc/vtium/vtium.conf` and `/etc/vtium/batch.conf`. Every other setting keeps its default: vtium renders through the GPU, picks a font size to suit the display, and lets you switch fonts. [Configuration keys](config.md) describes every key.

## Start vtium and log in

Press Ctrl+Alt+F3, or the key of another VT from 2 to 6 that your desktop does not use. Nothing runs on that VT yet, so logind starts vtium there, and vtium shows its login prompt. From the next boot, vtium starts by itself on each of its VTs.

If the old text login prompt shows instead, a getty already runs on that VT. Go back to your desktop and run this command, which ends that login prompt and starts vtium in its place:

```
$ sudo systemctl start vtium@tty3.service
```

## Try other fonts

In a session in vtium, these commands switch the console to another font set until you log out:

```
$ vtium config use-frit jetbrains-mono
$ vtium config use-frit monoid
$ vtium config use-frit cullet-mono
```

Ctrl+= zooms in and Ctrl+- zooms out, also until you log out.

To compare font sets side by side, run `vtium config tui`. Press `3` for the Looks tab, highlight a font set, and press `t`. The console shows that font set for 10 seconds. Press `k` to keep it until you quit the tool, or any other key to put the old font back.

To keep a font set for every login, set it as `font-name` and install the draft:

```
$ vtium config set font-name jetbrains-mono --install
```

The next login on each VT uses the new font. Another set from the catalog, such as `fira-code`, needs its font files activated first. [Choosing a font with vtium config](fonts.md#choosing-a-font-with-vtium-config) gives the steps.

## Restore the machine

From your desktop, stop vtium on every VT, remove the packages, and remove the files that vtium-config wrote. Stopping vtium ends every session that runs in it:

```
$ sudo systemctl stop 'vtium@*'
$ sudo pacman -Rs vtium vtium-fonts vtium-terminfo ttf-nerd-fonts-symbols-mono
$ sudo rm -r /etc/vtium /var/lib/vtium
$ rm -r ~/.config/vtium ~/.local/state/vtium
```

The removal gives every VT back what it ran before vtium. `-s` also removes the fonts that `vtium-fonts` pulled in, unless another package needs them. If you kept a copy of `/etc/vtium`, put it back with `sudo cp -a /root/vtium-before /etc/vtium` instead of removing that folder. SDDM, your desktop and the kernel's console need no restore, because vtium does not change them.

To keep vtium installed and give the VTs back, run `sudo vtium console disable --all`, which takes effect at each VT's next start.

To boot once without vtium, add `vtium.enable=0` to the kernel command line in your boot loader. vtium then exits as soon as it starts, and a getty's login prompt starts on each of its VTs. [Find out what went wrong](troubleshooting.md) covers the other ways out of a VT where vtium misbehaves.
