# Find out what went wrong

This guide helps when vtium misbehaves on a VT or does not do what you expected. It first gets you off that VT, and then shows where to find the reason: in what `vtium console status` says each VT runs, and in vtium's log.

In the commands below, `#` marks a command you run as root, for example with sudo, and `$` a command you run as your own user.

## Getting out

When vtium misbehaves on a VT, these steps get you out, in this order:

1. Switch to another VT, or to your desktop's, with Ctrl+Alt and a function key from F1 to F12. On its own VT, vtium reads the keyboard itself, and the kernel switches VTs only when vtium asks it to. So when vtium has stopped answering keys, those keys cannot take you off its VT, and you take the next steps from another machine over ssh instead.

2. Restart that VT's vtium, from another VT, a desktop terminal or another machine over ssh. A restart ends every session on the VT. On VT 3:

    ```
    # systemctl restart vtium@tty3.service
    ```

    The development install's unit is `vtium-dev@tty3.service` ([The development VT](vt-review.md#installing-vtium-on-a-vt)).

3. Take vtium off the VT at once by taking the VT back and starting its getty there:

    ```
    # vtium console disable 3
    # systemctl start getty@tty3.service
    ```

    `disable` decides what starts on VT 3 from now on ([Giving vtium a VT](install.md#giving-vtium-a-vt)). Starting the getty stops vtium on VT 3, because vtium's unit conflicts with the getty's, and that ends every session there. On the development VT, `vtium console disable` refuses, since the development install holds the VT, and the `systemctl start getty@tty5.service` line alone stops `vtium-dev` there; `make vt-uninstall VT=5` takes the install off for good ([The development VT](vt-review.md#installing-vtium-on-a-vt)).

When vtium fails, systemd starts it again. From systemd 254 on, each failure also starts vtium's fallback, which starts the VT's getty unless the head file's policy is `closed`, so under an open policy you get a plain login prompt at the first failure instead of a dead screen ([A VT that shows a getty](#a-vt-that-shows-a-getty)).

## Reading vtium console status

`vtium console status` first says which console logind starts on a VT with no link of its own, and which display manager the machine has. Then it prints a line for each VT:

- **at boot** names what systemd starts on the VT at boot, such as `getty@tty1`, `vtium@tty1` or `display manager (sddm.service)`. `getty recorded` means that `vtium console` disabled the VT's getty and keeps a record of it, which `disable` puts back. `no getty recorded` means that `vtium console` recorded that no getty was enabled on the VT, so `disable` enables none there. `vtium@ttyN masked` means someone masked vtium's unit there.
- **when switched to** names what logind starts when you switch to the VT while nothing runs there. It shows `-` on VT 1 and on VTs 7 and above, where logind starts no console ([Giving vtium a VT](install.md#giving-vtium-a-vt)).
- **running** names what runs on the VT now, a graphical session included.

A VT whose **at boot** column names a getty keeps that getty when you switch to it, whatever the **when switched to** column says, because the getty already runs there.

## A font that does not load

vtium draws its built-in font when it cannot draw the frit, and logs why. Read the log of that VT's vtium:

```
# journalctl -b -u vtium@tty2.service
```

A line such as `the font monoid is not drawn (...)`, or `the font's glyphs are not generated (...)`, gives the reason. Check these in turn:

- vtium finds a frit by its file's name: `font-name=monoid` reads `/etc/vtium/fonts/monoid.vtfrit`. A file named otherwise, such as `Monoid.vtfrit`, is not found, and vtium then looks for a kbd font named `monoid`.
- A source the helper cannot find is skipped without a warning, and the next source draws in its place. When everything draws in the emoji font or the built-in font, the helper did not find your font: it searches only `/usr/share/fonts` and `/usr/local/share/fonts`, as user `nobody` ([An outline font](fonts.md#an-outline-font)).
- The font file's own name table must give the family the frit names, as its typographic or its legacy family name; the file's own name does not matter. `fc-scan --format '%{family}\n' FILE` prints the families a file gives, and `font:` takes any of them, with case, spaces, `-` and `_` ignored. When several files give the family, the helper takes the one whose style is Regular.
- An unknown key or a malformed line refuses the whole frit, and the log names the file and the line.
- A change to the frit or to `vtium.conf` waits for vtium's next start on that VT ([Configuration](configure.md)).

`vtium check-config` also reports a frit whose `cell=` line vtium cannot read.

## A VT that shows a getty

The unit's fallback, `vtium-getty@ttyN.service`, starts a getty on a VT where vtium fails, so that you are never left at a dead screen. A login prompt where you expected vtium therefore usually means vtium failed. vtium exits with a failure, for example, when no display appears within `display-wait`, 10 seconds by default. Find the reason in its unit's state and log:

```
$ systemctl status vtium@tty2.service
# journalctl -b -u vtium@tty2.service
```

A getty that ran on the VT before you gave it to vtium also keeps the VT until it stops. Log out there and start vtium, as in [Giving vtium a VT](install.md#giving-vtium-a-vt).

## A VT that shows nothing

- vtium turns its displays off after `dpms-timeout`, 600 seconds by default, with no key press or pointer action. Press a key.
- Under a head file whose policy is `closed`, the fallback starts no getty, since a getty's login would take any keyboard, and vtium waits for a display however long it takes. Run `vtium check-config` and read the log.
- A VT whose link still names `vtium@.service` after the package was removed gets no console at all. The [vtium-arch README](https://github.com/vtium/vtium-arch#removing-vtium) gives the commands that clean such links up.

## A VT that stays black after you switch back to it

You switch from the desktop back to a VT where vtium runs, and the screen stays black, and a key press does not bring it back. This is a known defect, which [issue #249](https://github.com/vtium/vtium/issues/249) follows. Until it is fixed, restart that VT's vtium ([Getting out](#getting-out)). The restart ends every session on the VT. If the screen stayed black until you rebooted, collect a report of the boot before with `vtium-config report --previous-boot --save` ([Collecting a report and filing an issue](#collecting-a-report-and-filing-an-issue)).

## Pictures that do not show

A program that draws sixel or kitty graphics pictures shows nothing, or shows text where the picture should be. Check these in turn:

- vtium draws pictures only in a build with the `images` feature. `vtium --version` names `images` on its `features` line for such a build, and so does the vtium part of `vtium-config report` ([Collecting a report and filing an issue](#collecting-a-report-and-filing-an-issue)). The `vtium` and `vtium-git` packages are built with it, and the minspec packages are not ([Arch Linux](install.md#arch-linux)).
- `image-storage-limit` must be above 0, since `0` turns pictures off, and neither `sixel` nor `kitty-graphics` may be turned off ([config.md](config.md)).
- The program must see that the terminal draws pictures. vtium tells a program that asks: its reply to DA1, the terminal's device attributes query, names sixel, and it answers kitty's graphics query. Some programs do not ask, and decide from `TERM` alone, which is `xterm-vtium` in vtium, so they offer pictures only to a terminal they know by name. Tell such a program which kind of picture to draw where it has an option for it, as `chafa -f kitty` does.
- A kitty graphics picture sent as a file, as shared memory or as PNG needs the image helper, `/usr/bin/vtium-image`, which a build with images installs. When vtium cannot start the helper, its log says why ([A font that does not load](#a-font-that-does-not-load) shows how to read the log).

## vtium console refuses a VT

Each refusal names what holds the VT and how to free it: `systemctl disable --now kmsconvt@ttyN.service` for `kmscon`, `make vt-uninstall VT=N` for the development install, `systemctl unmask vtium@ttyN.service` for a masked unit. vtium never takes a VT from a display manager or a graphical session; give vtium another VT.

## Collecting a report and filing an issue

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
