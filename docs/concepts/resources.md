# Active resources and the trust rule

A package installs files that a program can use: fonts, color themes, cursor themes. On a desktop, a terminal emulator runs as you and can use any of them, your own fonts included. vtium is different. It runs as root on the console where people log in, so the person who runs the machine decides which of those files vtium may read. This page explains how vtium keeps that choice, the list of active resources, and the rule each file on the list must pass.

## Resources

A resource is a file that vtium draws from but that holds no settings. vtium has five kinds:

| Kind | What it is | Where it usually lives | How to use it |
|---|---|---|---|
| font | a bitmap or outline font file that a frit (a font set file) names | `/usr/share/fonts`, `/usr/local/share/fonts`, vtium's own `/usr/share/vtium/fonts`, and kbd's console-font folders | [Active fonts](../fonts.md#active-fonts) |
| color scheme | a palette file, `NAME.palette`, which sets the 16 ANSI colors and the default colors | `/usr/share/vtium/palettes` and `/etc/vtium/palettes` | [Color schemes](../palettes.md) |
| color map | a `.cube` file, a color table that a vtlook's `cube` filter reads | `/etc/vtium/looks` | [Color tables from .cube files](../looks.md#color-tables-from-cube-files) |
| shader | a `.vtshader` file for the spatial filters, such as scanlines | `/usr/share/vtium/shaders` and `/etc/vtium/shaders` | [the frame filters issue](https://github.com/vtium/vtium/issues/219) |
| pointer theme | vtium's own pointer pictures, or a desktop cursor theme's folder | `/usr/share/vtium/pointers.svg`, `/usr/share/icons` and `/usr/local/share/icons` | the `pointer-theme` key in [Configuration keys](../config.md#keyboard-and-mouse) |

vtium can already list and activate shaders, but the spatial filters that read them are not built yet.

vtium.conf, the frits and the vtlooks are not resources. They are settings, and vtium reads them under the check that [Security](security.md#a-configuration-only-root-can-change) describes for vtium.conf.

## The list of active resources

vtium draws only from the active resources. A resource is active while `/etc/vtium/batch.conf` lists it. The file holds one absolute path on each line, and a line that starts with `#` is a comment. A pointer theme is listed by its folder, such as `/usr/share/icons/Adwaita`, and every file in that folder counts as active.

vtium treats a resource that the list leaves out as a file that does not exist:

- A font's characters come from the next font in the frit, and in the end from vtium's built-in font. A frit that names a font by its family finds only the active files of that family.
- A look whose color scheme is missing draws the color scheme that the top of vtium.conf names, and in the end `legacy`.
- A vtlook whose color map is missing fails, and its heads draw the built-in vtlook, which changes no pixel.
- A missing pointer theme leaves the pointer with vtium's own pictures.

The built-in font is always active, and the list never names it.

## What a fresh install activates

The package installs no `batch.conf`. While the file does not exist, vtium uses the list of a fresh install:

- the built-in font;
- the font files that vtium installs in `/usr/share/vtium/fonts`: Terminus in nine sizes from 12 to 32 pixels, regular and bold, GNU Unifont, and Cullet Mono's regular and bold faces;
- the console font that `FONT=` names in `/etc/vconsole.conf`, found in kbd's console-font folders;
- every color scheme in `/usr/share/vtium/palettes`;
- vtium's pointer pictures, `/usr/share/vtium/pointers.svg`.

So the default font set, `terminus`, and the color schemes vtium installs work with no setup. The list holds no color map, no shader and no other font. `vtium check-config` notes that the file is missing.

vtium also uses the list of a fresh install when `batch.conf` fails the owner check that vtium.conf passes, holds more than 64 KiB, or cannot be read. A list that someone other than root could have written says nothing about what root chose. `vtium check-config` names the file and says why vtium ignores it.

Once `batch.conf` exists, vtium uses only the paths it lists. vtium-config writes the file the first time you install a change to what is active. It starts from the list of a fresh install and applies your change, so the default fonts stay active. A file you write by hand replaces that list, so list the fonts, color schemes and pointer file that vtium installs too. [Active fonts](../fonts.md#active-fonts) shows the commands.

## The trust rule

Listing a file is not enough. vtium also opens each resource under the trust rule, which asks four things:

- Root owns the file and every folder above it.
- Neither the group nor other users can write the file or any of those folders.
- Neither the file nor any folder on its path is a symbolic link.
- The filesystem that holds the file is mounted at boot and stays mounted while the system runs.

The last check fails a network filesystem, such as NFS or SMB, and a FUSE filesystem, which a program serves. It also fails an automounted filesystem and a removable disk. vtium counts a disk as removable when the kernel marks it so, or when it sits on a USB bus. A disk that holds the root filesystem is the exception, as on a machine that boots from a USB stick. Its other filesystems, such as `/home` on the same stick, pass too.

A desktop cursor theme holds symbolic links, such as Adwaita's `xterm`, which points to `text`. Inside a theme's folder, vtium follows a link that stays inside that folder, and refuses a link that leaves it.

vtium checks the file it actually opened, so nobody can swap the file between the check and the read. A file that fails any check counts as a file that does not exist, just like a file the list leaves out. vtium logs the file and the check it failed, and `vtium check-config` reports both. When you run vtium by hand as your own user, as the tests do, vtium accepts your files as well as root's.

## Why vtium has the rule

vtium runs as root, and every session on its VTs sees what it draws. Two things follow from that.

First, only root should decide what the console draws. A file you own, such as a font in `~/.local/share/fonts`, can change while the console uses it, through any program you run. A file on a USB stick or a network share can vanish or change too, when someone pulls the stick or the server changes the file. The trust rule leaves vtium only files that root put in place and that stay as root left them. The list then records which of those files root chose.

Second, font files and color tables have complex formats, and the code that reads such formats is where programs most often have security defects. The font helper already parses fonts in a sandbox with fewer rights, as [Security](security.md#root-for-the-console-fewer-rights-for-the-helpers) describes. The rule adds a second line of defense: a font that the list does not name never reaches the parser at all, even inside the sandbox.

## When a change takes effect

vtium reads `batch.conf` when it starts, and again each time a session ends, before the next login on that head. When the list changed, vtium brings each part that draws from it up to date:

- The font helpers take the new list, and vtium draws every glyph again.
- vtium reads each color scheme again at its next use.
- The head chooses its vtlook again, so a color map you activated or deactivated takes effect.
- The pointer pictures load again when the pointer theme's place in the list changed.

So a resource you activate or deactivate takes effect at the next login, with no restart of vtium. Each head catches up at its own next login. A change to vtium.conf itself, such as the color scheme a look names, still waits until vtium next starts on the VT. See [The file](../configure.md#the-file) for details.

## Activating a resource

You change the list with vtium-config, which keeps the change in your draft beside vtium.conf and installs both together through sudo. The commands are `vtium config activate`, `vtium config deactivate` and `vtium config active`. vtium-config's Fonts tab activates fonts, its Effects tab activates color maps and shaders, and its Looks tab activates color schemes and pointer themes. [Choosing what vtium may use](../install.md#choosing-what-vtium-may-use) gives the steps.

A font in your home folder cannot be activated, because root does not own it. vtium-config can install it as a system font first, with `vtium config fonts --install-system FILE`, which copies it into `/usr/local/share/fonts` as root.

See [the trust rule issue](https://github.com/vtium/vtium/issues/438) for the work on the list and the rule.
