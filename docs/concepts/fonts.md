# Fonts, generated at run time

This page describes how vtium gets the glyphs it renders. A glyph is the image of one character in a font. These parts are all built: the font helper, the console's per-boot store, the built-in fonts, switching, preview, and new-frit.

The console does not read font files itself. Two parts do that work for it:

- A **frit**, or font set file, names a font's sources.
- The **font helper** is vtium-config's serve command, which runs in a sandbox. It reads the frit's sources and rasterises them, which means it turns each character into a bitmap. It sends the console those glyphs at the size of the console's character cell, the box on the screen that holds one character. The glyphs come ready to draw, with their map from Unicode characters to glyphs and their metrics, the measurements that place each glyph in the cell.

The console checks each answer from the font helper. It keeps the glyphs in memory until the machine restarts, and draws them. Nothing is rendered ahead of time, and no glyph lasts past a restart.

The font terms come from glassmaking. [why-vtium.md](../why-vtium.md) explains why, for a newcomer.

The figure follows a font from the moment someone chooses it to the console. Each command in the figure is a vtium-config command.

```mermaid
flowchart LR
    P["preview<br>try many fonts<br>(the user)"] --> R
    W["new-frit: from fonts named<br>or a template<br>(the user)"] --> R
    H["by hand"] --> R
    R["frit<br>names the sources<br>(written by root)"] --> HE["serve, the helper<br>sandboxed,<br>never root"]
    HE -->|"glyphs, checked"| ST[("the per-boot store<br>in vtium's memory")]
    ST --> V["vtium"]
    B["built-in fonts<br>Unifont, Unscii-8"] -->|"the first frame<br>and the fallback"| V
    S["use-frit<br>(the user,<br>in the session)"] -->|"OSC 50"| V
    classDef step fill:#2d7d9a,color:#ffffff,stroke:#94a3b8
    classDef core fill:#7c3aed,color:#ffffff,stroke:#94a3b8
    classDef store fill:#2d8e5e,color:#ffffff,stroke:#94a3b8
    classDef input fill:#fbbf24,color:#1a1a1a,stroke:#94a3b8
    classDef user fill:#475569,color:#ffffff,stroke:#94a3b8
    class HE step
    class V core
    class ST,B store
    class R input
    class S,P,W,H user
```

## Try many, keep one

Someone choosing a font tries many and keeps one. `vtium config preview` renders a sample of each font it names, the way the font helper would render it. It uses the size that its `--size` option gives. Without `--size`, it uses the font size in vtium.conf. In a terminal that shows kitty graphics images, it shows the sample as an image. In other terminals, it prints a line of text for each font. It writes nothing.

## The frit

A font is declared in a **frit**. The name comes from glassmaking, where frit is the ground glass that a glassmaker melts again. A frit is a small text file, `/etc/vtium/fonts/NAME.vtfrit`. It names the font's sources in order, and a later source fills in only the characters that the earlier sources do not have. The frit also names the character cell the font is drawn in.

A frit's sources can be any of these:

- console fonts
- font files
- the kernel's own font
- outline fonts, color emoji among them, which the font helper finds among the fonts installed on the machine by their family name

There are three ways to write a frit, and all three give the same file:

- You can write it by hand.
- `vtium config new-frit FONT...` makes a frit of the fonts you name, in the order you name them. `vtium config new-frit` with no fonts, run at a terminal, opens vtium config's full-screen tool at its new-frit view.
- `vtium config new-frit --template` prints a frit with each key explained. `vtium config new-frit --from FILE` installs the frit you wrote from that template. This way, a script or a package can make a frit without answering questions.

Installing a frit asks for sudo once. The tool shows you the command before it runs it.

`vtium-config --help about` draws the tool's name in glass. See [the glass banner issue](https://github.com/vtium/vtium/issues/112).

Later, vtium-config may find installed fonts that match the looks people choose, without a frit written for each. A look is a named set of font settings in vtium.conf, such as a font and its size. That direction is not decided yet. See [the issue about matching installed fonts](https://github.com/vtium/vtium/issues/117).

## Drawing at run time

When vtium starts, it draws the first frame at once in the built-in font, in the character cell that the frit states. The frit's glyphs then replace the built-in ones, character by character, as the font helper sends them. The font helper sends the characters on the screen first. While the console asks for nothing else, it goes on to printable Latin characters and box-drawing characters. After those, it sends the rest of the characters the frit covers.

The glyphs live in a per-boot store, which lasts until the machine restarts. The store's limit depends on how much memory the machine has. When the store reaches its limit, the font helper stops filling it. Only a request for characters on the screen makes room in a full store. It evicts the glyphs least recently shown first.

The font helper starts the first time the console needs it, and exits when it has nothing to do. If it crashes, vtium starts it again, up to a limited number of times per boot.

## The fallback and switching

The console uses the built-in fonts in two cases, and it logs the reason. In the first case, the configured font names no frit, and no font that vtium can make a frit from on the fly. In the second case, the font helper is not available.

The built-in fonts are compiled into vtium. They are Unifont, and Unscii-8 for character cells under 16 pixels high. They also draw each character that a frit does not have.

vtium-config's use-frit command switches the running console to another frit or to a built-in font. It does this through the terminal, with xterm's font control sequence, OSC 50. vtium acts on that sequence while its allow-font-ops setting is on, as it is by default.

A switch lasts until the session it came from ends. The session ends when its program exits, when its console closes, or when vtium hangs it up. vtium then returns the console to the font the configuration gives.
