# vtium's additions to the replay interface

Interface 1.1 is `corpus/INTERFACE.md`, interface 1.0 as the C reference left it, with the additions below. Each is a command, a feature or a line only a new command prints, so it raises MINOR under 1.0's "Versioning and features", and every golden stays as it is. The runner and the probe runner read the fenced blocks here after 1.0's: this `describe` block's version line replaces 1.0's, and its view, feature and command lines follow 1.0's lines of the same kind; this `grammar` block's lines join 1.0's, using its fragments.

## The clipboard

A program sets the clipboard with OSC 52 (`osc.52.write` in ADR-1000), and a host takes what it set. The reference host takes it only when asked, with the `clipboard` command.

| Command | Keys | Feature | What it does |
|---|---|---|---|
| `clipboard` | none | `clipboard` | Takes the clipboard as a host does, and prints `test clipboard HEX`, HEX being the bytes the last OSC 52 write set since the previous `clipboard`, two lowercase hex digits each, or `test clipboard none` when no write set any. |

| Feature | What declaring it means |
|---|---|
| `clipboard` | OSC 52 writes set the clipboard; `clipboard`. |

```describe
interface 1.1
feature clipboard
command clipboard clipboard
```

```grammar
line test * test clipboard (?:none|(?:[0-9a-f]{2})+)
```
