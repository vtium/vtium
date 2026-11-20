# Looks and effects

In a desktop terminal, you choose a font, a size and a color theme in its preferences, and every window takes them. vtium can drive several displays, each with its own terminal, so it keeps those choices in named bundles called looks. Each head, a terminal on one or more displays, takes one look. This page explains what a look holds, how a color scheme differs from an effect, and how vtium draws a frame through an effect.

## What a look holds

A look is a `[look NAME]` section of vtium.conf. A head names its look with `look = NAME`, and a head that names none takes the look `default`. A look holds:

- a font set, a frit (a font set file) that `font-name` names, with its size, `font-size`, or its character cell, `cell`;
- the other font settings, such as `font-hinting`, `font-line-spacing` and `font-ligatures`;
- a color scheme, which `palette` names;
- a vtlook, which `vtlook` names, the file that chooses the head's effects;
- the login picture and the display settings `display-depth` and `display-dpi`.

A setting that a look leaves out comes from the top of vtium.conf. So you can set a color scheme once for every head, and give one look its own. For example, a head on a large monitor could take a look with a bigger font and an amber effect, while the laptop's panel keeps the default look.

Every look draws the pointer theme that the top-level `pointer-theme` key names. A look cannot pick a pointer theme of its own yet. See [the pointer theme issue](https://github.com/vtium/vtium/issues/442).

The fonts, the color scheme and the files a vtlook reads must be active, which [Active resources and the trust rule](resources.md) explains. [The sections of vtium.conf](../head-file.md#reading-the-sections) describes the look's section.

## Two ways to change the colors

vtium changes the colors on screen at two different points.

A color scheme decides what the numbered colors are. Programs name most colors by number, such as color 1 for red or the default background, and the color scheme gives each number its RGB value. vtium resolves each character cell through the color scheme as it draws the text. A program can change those colors in its own console with escape sequences, such as OSC 4, and a reset returns them to the color scheme's. [Color schemes](../palettes.md) lists the color schemes vtium installs, and the file format of your own.

An effect changes the finished picture, after vtium has drawn the text. It reaches everything on the display: the text, the border, the pointer and vtium's own notices. A program cannot change it. For example, an amber effect shows a Dracula color scheme in shades of amber.

## A vtlook is a chain of filters

A vtlook (a file of frame filters) chooses the effects for a head. It is a file, `NAME.vtlook`, and each of its sections names one frame filter, a step that changes the finished frame. vtium has five color filters, which change each pixel's color from that color alone:

- `gamma` lightens or darkens the middle tones.
- `contrast` spreads the tones apart or pulls them together, and shifts their brightness.
- `tint` turns the screen toward one color, like a single-color phosphor. Its `floor` lifts the dark colors above black, so dark blue text stays visible under amber.
- `color-blind` shows the screen as a viewer with a color vision deficiency sees it, or corrects the colors for that viewer.
- `cube` maps each color through a color map, a `.cube` file that a color grading or calibration tool saves.

vtium applies the filters in the order of their sections, so a vtlook can grade the colors warm and then tint them amber. A vtlook can also hold a `[requires]` section, which names the displays it suits, such as those of 24-bit color. A display that does not meet the requirements draws without the effect, and the head's other displays keep it. Spatial filters, which read the pixels around each pixel, such as scanlines, are not built yet. See [the frame filters issue](https://github.com/vtium/vtium/issues/219).

[Vtlook files](../looks.md) gives the syntax, each filter's parameters, the requirements and the errors.

## The presets and No filter

vtium installs six vtlooks in `/usr/share/vtium/looks`. Three are phosphor presets, `amber`, `green` and `white`, which tint the screen like a monochrome monitor. Three are corrections for color vision deficiencies: `protanopia`, `deuteranopia` and `tritanopia`. Your own vtlooks go in `/etc/vtium/looks`, and a file there that takes a preset's name replaces the preset.

The built-in vtlook, `default`, holds no filter and changes no pixel. vtium-config calls it No filter. A head whose look names no vtlook draws it, and so does every head whose vtlook fails. A vtlook fails on an error in its file, or when a color map it reads is missing or not active. vtium then logs why. No file can take the name `default`, so the vtlook that every failure falls back to never fails itself.

vtium-config's Effects tab lists No filter, the presets and your own vtlooks, then the color maps and the shaders. As you move through the list, it shows a picture of the console with the highlighted effect applied, and leaves the live console alone.

## How vtium draws a frame through a vtlook

vtium merges a vtlook's color filters into one color table, a lattice of 33 steps in each of red, green and blue. Each point of the table holds what the filters, in order, make of that color. However many filters the vtlook holds, each pixel then costs one lookup in the table. A color between the table's steps is blended from the eight points around it. The head builds its table once, and all its displays share it.

The table applies to the finished frame, after vtium has rotated the frame for a turned display:

- On the CPU path, vtium looks up each pixel as it writes the pixel to the display's buffer. It remembers the last color it looked up, so a run of one color, such as a background, costs one lookup.
- On the GPU path, vtium uploads the table as a 3D texture, and the graphics card looks up each pixel as it draws the frame.

Both paths read the same table, so a head looks the same on either. While a head's vtlook holds a filter, vtium draws the pointer into the frame instead of on the display's cursor plane, so the effect reaches the pointer too.

Frame filters are a build feature, `filters`. A build without it, such as the smallest build, reads the `vtlook` key and draws every head with the built-in vtlook. `vtium --version` names `filters` on its `features` line when the build has it.

## When a change takes effect

vtium reads vtium.conf when it starts on a VT. So a change to which color scheme or vtlook a look names takes effect when vtium next starts there. See [How an install reaches the VTs](../configure.md#how-an-install-reaches-the-vts) for details. Activating or deactivating a color scheme or a color map takes effect sooner, at the head's next login, as [When a change takes effect](resources.md#when-a-change-takes-effect) explains.
