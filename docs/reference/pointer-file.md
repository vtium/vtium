# The pointer file

You can replace vtium's mouse pointer pictures with your own. Set `pointer-theme` in vtium.conf to an SVG file that you drew, or to the folder of a desktop cursor theme:

```
pointer-theme = /etc/vtium/my-pointers.svg
pointer-theme = /usr/share/icons/Adwaita
```

The path must be absolute. vtium looks at what is at the path when it starts. A regular file is an SVG pointer file, and a folder that holds a `cursors` folder is a cursor theme. vtium searches no icon folder and follows no `Inherits=` line, so name the folder that holds the cursors.

vtium draws its own pictures until the pictures from the file are ready, and for any shape the file lacks. When `pointer-char` is set too, its character keeps the default shape, and the file's pictures serve the other shapes.

## The ten shapes

A program can ask for a pointer shape with OSC 22. vtium knows ten shapes, and a pointer file names each by the same name:

| Name | What it shows |
|---|---|
| `default` | The arrow |
| `text` | An I-beam, over text |
| `pointer` | A hand, over a link |
| `crosshair` | A thin cross, for picking a point |
| `wait` | An hourglass, while a program is busy |
| `progress` | The arrow with a small hourglass, while a program is busy but still takes input |
| `not-allowed` | A circle with a slash |
| `move` | Four arrows |
| `ew-resize` | A double arrow pointing left and right |
| `ns-resize` | A double arrow pointing up and down |

## An SVG pointer file

vtium installs its own pictures as `/usr/share/vtium/pointers.svg`. The easiest start is to copy that file, change its symbols in an SVG editor, and point `pointer-theme` at your copy.

Each shape is a `<symbol>` element anywhere in the file. A symbol has three attributes, and no other element in the file may have the same `id`:

- `id` is the shape's name from the table above.
- `viewBox` sets the symbol's coordinates and its proportions.
- `data-hotspot="x y"` is the point the pointer names, such as the arrow's tip. It uses the viewBox's coordinates and must lie inside the viewBox.

```xml
<svg xmlns="http://www.w3.org/2000/svg">
  <symbol id="default" viewBox="0 0 11 18" data-hotspot="0 0">
    <polygon points="0,0 0,16 4,12 7,18 9,17 6,11 11,11" fill="#fff" stroke="#000"/>
  </symbol>
</svg>
```

A symbol can use anything else the file defines, such as a gradient in its `<defs>` or a style sheet. Nothing outside the symbols is drawn.

vtium draws each picture one character cell tall, but at least 8 pixels and at most 64. The width follows the viewBox's proportions. A picture that would be wider than 64 pixels is drawn 64 pixels wide, and shorter. The hotspot lands on the pixel under its point. When you zoom, vtium draws the pictures again at the new cell height. It keeps the pictures of the 16 cell heights used most recently.

Shapes draw with smooth edges unless the file asks for crisp ones. Put `shape-rendering="crispEdges"` on the `<svg>` element, as vtium's own file does, to fill each pixel whose centre lies inside a shape.

The file may be at most 1 MiB. Text, fonts and embedded images in the file do not draw, so a symbol that holds only those draws nothing, and vtium draws its own picture for that shape.

## A cursor theme

A cursor theme is a folder, such as `/usr/share/icons/Adwaita`, whose `cursors` folder holds one file for each cursor. vtium tries these files for each shape, in order, and takes the first one that it can read and draw:

| Shape | Files vtium tries |
|---|---|
| `default` | `default`, `left_ptr` |
| `text` | `text`, `xterm` |
| `pointer` | `pointer`, `hand2` |
| `crosshair` | `crosshair`, `cross` |
| `wait` | `wait`, `watch` |
| `progress` | `progress`, `left_ptr_watch` |
| `not-allowed` | `not-allowed`, `crossed_circle` |
| `move` | `all-scroll`, `fleur` |
| `ew-resize` | `ew-resize`, `sb_h_double_arrow`, `col-resize` |
| `ns-resize` | `ns-resize`, `sb_v_double_arrow`, `row-resize` |

A cursor file holds each picture at several sizes. vtium picks the size nearest the height of a character cell. Set `pointer-size` to the size your desktop's cursor size setting names to pick that size instead:

```
pointer-size = 24
```

vtium never scales a theme's picture, so its outlines stay sharp. It uses sizes up to 64 pixels a side, and keeps at most four pictures of 64 × 64 pixels for each shape. A busy cursor shows its first frame and does not move. A theme's pictures keep their own colors.

A cursor file may be at most 16 MiB, with at most 4,096 entries in its table of contents.

## Who reads the file

vtium reads a pointer file only when root owns it, or the user vtium runs as. Neither the group nor other users may be able to change the file or any folder on the way to it. These are the same checks that vtium.conf and the font sets pass.

The console never reads a pointer file's contents itself. It passes the file to the font helper, which draws the pictures in its sandbox, as it draws the font's glyphs. The console then checks each picture before it shows it.

## What vtium logs

vtium logs one line for each problem and then draws its own picture for the shapes affected:

- A path that is neither an SVG file nor a folder with a `cursors` folder: "The pointer file /usr/share/icons/default is neither an SVG file nor a folder that holds a cursors folder, so vtium draws its own pointer pictures."
- A file vtium does not read, because it is too long or others can change it. The line names the file, and for an ownership problem the command that fixes it.
- A picture the font helper cannot draw: "The font helper cannot draw the wait shape from /home/kim/pointers.svg, because its symbol has no data-hotspot. vtium draws its own picture for that shape."
- A picture that fails vtium's checks: "vtium does not use the wait picture from /home/kim/pointers.svg, because the font helper sent a picture in which no pixel shows. It draws its own picture for that shape."
- A font helper that fails or stops answering while it draws the file. vtium keeps the pictures it has, asks for no more, and logs the reason in the line.
- Shapes the file lacks, at info level: "The pointer file /usr/share/icons/Oxygen_White has no picture for the ew-resize and ns-resize shapes, so vtium draws its own pictures for them."

`vtium check-config` checks that `pointer-theme` is an absolute path. It does not open the file, because the console reads the file where it runs.

See [Configuration keys](../config.md) for `pointer-theme`, `pointer-size` and `pointer-char`.
