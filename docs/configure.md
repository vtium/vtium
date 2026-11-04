# Configure vtium

This guide changes how vtium behaves through its configuration file, `/etc/vtium/vtium.conf`, and its head file, `/etc/vtium/heads.conf`. vtium reads the keys the C reference's `kmscon.conf` took, and adds its own; [config.md](config.md) lists every key with its default and its bounds, and [Set up heads](heads.md) gives a complete head file for each common setup.

In the commands below, `#` marks a command you run as root, for example with sudo, and `$` a command you run as your own user.

## Write vtium.conf

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

vtium reads its files once, when it starts: `vtium.conf`, the head file and the frits. A change reaches a VT at that VT's next start, so log out of the VT and restart its vtium, as in [Upgrading and removing](install.md#upgrading-and-removing).

`vtium config` changes the font keys of vtium.conf and the profile each head of the head file draws with, and shows each changed line before it saves ([Choosing a font with vtium config](fonts.md#choosing-a-font-with-vtium-config)).

The head file, `/etc/vtium/heads.conf`, decides which displays and keyboards make up each head, a terminal of its own for one or more displays ([heads.md](heads.md)). With no head file, vtium runs one head on every display. `vtium check-config` reads the configuration and the head file as a start would, applies nothing, and prints each problem it finds. Given a file, it checks that file in the head file's place, so you can check a new head file before you install it:

```
$ vtium check-config
$ vtium check-config ./heads.conf
```

It exits 1 when it finds a problem and 0 when it finds none.

## Blanking from a script

`vtium blank --vt N` turns off the displays of the vtium on VT N at once, and `vtium wake --vt N` turns them on again, as a key press does. Root and the user vtium runs as may send them:

```
# vtium blank --vt 3
```
