# Try vtium on one VT

In this tutorial, you install vtium and give it one spare VT. You log in there and try what it renders. Then you go back to your desktop, and take vtium off again. A VT, or virtual terminal, is one of the full-screen text screens that the Linux kernel keeps beside your desktop. You switch between them with Ctrl+Alt and a function key. The package's install gives vtium every free VT, from the next boot. In step 3, you take those VTs back and give vtium only one. vtium never takes the VT that your desktop runs on, so your desktop keeps running the whole time. At the end, vtium is off the machine again.

You need these things:

- about fifteen minutes;
- an Arch Linux machine with systemd;
- the `base-devel` package group, which `makepkg` needs to build anything;
- the password for sudo.

The commands below run in a terminal on your desktop, as your own user. Each command that needs root starts with `sudo`.

> **Not on Arch Linux?** vtium has no package for other distributions yet. Follow [Without a package](#without-a-package) at the end of this page instead. It runs vtium on a free VT straight from a checkout of its source, and it installs nothing.

## 1. Install the package

vtium's package comes from the AUR, the Arch User Repository, as a recipe that `makepkg` builds on your machine. The recipe checks that the maintainer signed the release it builds. So first import the maintainer's key, which the recipe carries. Then build and install:

```
$ git clone https://aur.archlinux.org/vtium.git
$ cd vtium
$ gpg --import keys/pgp/33F9B54FD2A4B6A4110749B3BA464FC9847A832A.asc
$ makepkg -sri
```

Here is what makepkg does:

1. It asks pacman to install the packages that vtium needs only to build, such as `cargo`, where your machine lacks them. The `-s` flag does this. sudo asks for your password, and pacman asks you to confirm. Answer yes.
2. It downloads vtium's source and builds it. This takes a few minutes.
3. It asks pacman to install two packages, because of the `-i` flag. They are `vtium`, the console itself, and `vtium-terminfo`, which tells programs what vtium's terminal can do. Answer yes again.
4. It removes the build-only packages that it installed at the start. The `-r` flag does this.

The install gives vtium every free VT, but it starts nothing. Each of those VTs changes at its next start, so nothing on your screen changes yet. The install message lists the VTs that vtium now holds. Step 3 takes them back, so do steps 2 and 3 before you next restart the machine.

## 2. Find your desktop's VT and a free one

Ask vtium what each VT runs now:

```
$ vtium console status
```

It first prints a line or two about the machine as a whole. The first line says which console logind starts on a VT that has no setting of its own. A display manager is the program that shows the desktop's login screen. Where the machine has one, the second line names it.

Then it prints a table, with a row for each of VTs 1 to 6, and for any other VT that has something on it. The first column is the VT's number. The other three columns say what systemd starts there at boot, what logind starts when you switch to it, and what runs there now. A dash means nothing. After the install, the **at boot** column names `vtium@ttyN` on each VT that vtium now holds. Your desktop's row says `graphical session of` and your user name, in its **running** column. That row is your desktop's VT. Remember its number, since you switch back to it in step 6.

Pick a VT that runs no graphical session and that nobody is logged in on. This tutorial uses VT 3. If VT 3 is your desktop's, use another number from 2 to 6 everywhere the steps say 3.

## 3. Give vtium VT 3

The install gave vtium every free VT. First take them all back, then give vtium VT 3 alone:

```
$ sudo vtium console disable --all
$ sudo vtium console enable --direct 3
```

`disable --all` gives every VT back what it had before the install. `enable --direct 3` makes vtium the console that starts on VT 3. It takes effect the next time something starts on VT 3, so nothing changes yet. Before it changes anything, vtium writes down what VT 3 had, so that it can put it back in step 7. It refuses a VT that a desktop or a display manager is using, so it cannot take yours. It ends by printing the command that starts vtium on VT 3 now, which is the next step.

## 4. Start vtium on VT 3

```
$ sudo systemctl start vtium@tty3.service
```

This starts vtium on VT 3, in place of the login prompt that was there. Nothing changes on your desktop.

## 5. Switch to VT 3 and log in

Press Ctrl+Alt+F3.

The desktop disappears, and you see a `login:` prompt, drawn by vtium. It is drawn in the machine's console font. Where the machine names no console font, it is drawn in vtium's own built-in font. Log in with your user name and password, as you would on any text console.

You are now in a shell that vtium renders. Try a few things:

```
$ ls --color /etc
$ printf '\e[38;2;255;120;0morange in 24-bit color\e[0m\n'
$ printf '\e[4:3mcurly\e[0m \e[4:2mdouble\e[0m \e[4:4mdotted\e[0m underlines\n'
$ printf '中文 日本語 한국어\n'
```

- The directory listing is in color. The second line is in exactly the orange that it names. The third line shows three styles of underline.
- The last line shows Chinese, Japanese and Korean, each character two character cells wide.
- Ctrl+= makes the text larger, and Ctrl+- makes it smaller again.
- Shift+PageUp scrolls back through what has scrolled off the top, and Shift+PageDown returns.
- Select some text with the mouse. The middle button then pastes it at the prompt.

You may see a plain login prompt drawn by the kernel instead of vtium's. That means vtium could not start, and a fallback gave you the kernel's login, so that the VT is never dead. Switch back to your desktop and read [Find out what went wrong](troubleshooting.md).

## 6. Switch back to your desktop

Press Ctrl+Alt and the function key of your desktop's VT, the number you found in step 2. Your desktop is there as you left it.

vtium keeps running on VT 3, and so does your shell there. Ctrl+Alt+F3 takes you back to it. Ctrl+Alt and your desktop's key return you to the desktop.

## 7. Take vtium off again

Switch to VT 3 once more, type `exit` to log out, and switch back to your desktop. Then give VT 3 back, and stop vtium there:

```
$ sudo vtium console disable 3
$ sudo systemctl stop vtium@tty3.service
```

`disable` puts back what VT 3 had before `enable` took it in step 3, from the VT's next start. `stop` ends vtium on VT 3 now. The next time you switch to VT 3, you find the machine's own login prompt there again. Now remove the packages. Their removal script runs `vtium console disable --all` once more. vtium holds no VT by then, so it gives nothing back:

```
$ sudo pacman -R vtium vtium-terminfo
```

vtium is now off the machine. A few things from this tutorial stay, and you can remove each one if you want to:

- the maintainer's key in your gpg keyring;
- the `vtium` folder you cloned from the AUR;
- the Rust crates that the build downloaded, in `~/.cargo/registry`;
- the folder `/var/lib/vtium/console/`. It is empty, because `disable` forgot VT 3's record once it had put VT 3 back.

## What next

- [Choose a font](fonts.md) sets the font that vtium renders in. `vtium config tui` lets you try each font on the live console before you keep one.
- [Install vtium and give it VTs](install.md) covers giving vtium every VT, upgrading, and moving to vtium from another terminal on a DRM/KMS VT.
- [What vtium does](features.md) lists what vtium renders. [Concepts](concepts/index.md) explains how its pieces fit together.

## Without a package

On a distribution with no vtium package, you can run vtium from a checkout of its source. This runs your own shell in vtium on a free VT, until you exit the shell. It installs nothing. You need systemd and logind, Rust 1.88 or later, clang, git, and the password for sudo. The first build also downloads a Debian system image that vtium links against.

```
$ git clone https://github.com/vtium/vtium.git
$ cd vtium
$ make vt-shell
```

`make vt-shell` builds vtium, picks a free VT, and asks for your sudo password. Then it prints which VT to switch to, and the key that brings you back. For example:

```
================================================================
  Switch to VT 5 now: Ctrl-Alt-F5    (Ctrl-Alt-F7 returns to this desktop)
================================================================
```

Press the first key it names. You are in your own shell, rendered by vtium, with no login. Try the commands from step 5 there.

Type `exit` to end the run. vtium stops, and `make` returns. The console that ran on that VT before the run starts there again. So on a VT where nothing ran, the screen stays blank. Press the key it named to return to your desktop.

If you want a login prompt on that VT, switch to it once more, and logind starts one. You can also start one yourself with `sudo systemctl start getty@tty5.service`, using the VT's number. Nothing was installed, so there is nothing to remove. Delete the checkout when you no longer want it.
