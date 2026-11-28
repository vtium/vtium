# The crates' interfaces

vtium's engine is split into crates, and each crate has an interface of its own. vtium-parse reads the bytes a program writes, vtium-term keeps the screen, and vtium-render draws it. vtium-font and vtium-image serve them, and the other crates build the console around them.

Each crate's reference is generated from its doc comments by [rustdoc](https://doc.rust-lang.org/rustdoc/), the tool that turns a Rust crate's doc comments into HTML pages. The reference has a page for each public item, and rustdoc's own search finds items across every crate. The site's search does not reach these pages.

[Getting started](getting-started.md) feeds bytes to vtium-term and reads the screen back. [Conventions](conventions.md) gives the rules every crate's interface follows, and says what is stable. Until vtium decides which interfaces it keeps, any interface may change in any release.

## The crates

| Crate | What it does | Main types |
|---|---|---|
| [vtium-parse](https://vtium.github.io/vtium/api/vtium_parse/) | Reads the bytes a program writes and turns them into terminal actions, which it hands to a consumer. | `Parser`, `Consumer`, `Params` |
| [vtium-term](https://vtium.github.io/vtium/api/vtium_term/) | Keeps the terminal's state: the screen, its lines and cells, the scrollback, the modes and the replies. It produces the frames a renderer reads. | `Terminal`, `Screen`, `Line`, `Cell`, `Frame` |
| [vtium-render](https://vtium.github.io/vtium/api/vtium_render/) | Turns frames into pixels on the CPU, or on the GPU behind the `gpu` feature. | `Renderer`, `Target`, `Canvas` |
| [vtium-font](https://vtium.github.io/vtium/api/vtium_font/) | Reads glyph sources: bitmap fonts, and the fonts built into the console. | `Font`, `Face`, `Glyph`, `Psf`, `Unifont` |
| [vtium-image](https://vtium.github.io/vtium/api/vtium_image/) | Reads the image protocols, sixel, kitty graphics and iTerm2's inline images, and keeps the image store. | `Sixel`, `Kitty`, `Iterm2`, `Store` |
| [vtium-image-helper](https://vtium.github.io/vtium/api/vtium_image_helper/) | The image helper `vtium-image`: a sandboxed program that reads the files a program names and decodes its images for the console. | the modules `serve`, `sandbox` and `decode` |
| [vtium-platform](https://vtium.github.io/vtium/api/vtium_platform/) | The console's platform: displays, VTs, seats, input, the pty and the event loop. | the modules `display`, `vt`, `input`, `pty` and `event_loop` |
| [vtium-settings](https://vtium.github.io/vtium/api/vtium_settings/) | vtium's configuration: the key table, vtium.conf and its sections, their readers and writers, and the rules that place each device in a head. | `Config`, `Identity`, `Placement`, `Error` |
| [vtium-devices](https://vtium.github.io/vtium/api/vtium_devices/) | Lists the displays and inputs of a seat, as vtium's head rules see them. | `Card`, `devices` |
| [vtium-broker-protocol](https://vtium.github.io/vtium/api/vtium_broker_protocol/) | The messages between the broker, which keeps root, and the drawer, which runs the rest of the console without it, and the socket that carries them. | `Request`, `Order`, `Link` |
| [vtium-font-helper](https://vtium.github.io/vtium/api/vtium_font_helper/) | The font helper, which generates the console's glyphs at run time in a sandbox, and the readers of the font files it draws from. | the modules `frit`, `glyphs`, `serve` and `sources` |
| [vtium-config](https://vtium.github.io/vtium/api/vtium_config/) | The tool `vtium-config`, which manages fonts and settings, with its command line and its TUI. It re-exports the modules of vtium-font-helper and vtium-config-core. | the modules `cli`, `tui` and `commands` |
| [vtium-config-widgets](https://vtium.github.io/vtium/api/vtium_config_widgets/) | vtium-config's TUI toolkit: ratatui's cells written as ECMA-48 bytes, rows of styled text, the console's sixteen colours, the shared list, and the decoder of keys, the mouse and the terminal's replies. | the modules `backend`, `canvas`, `color`, `listbox` and `input` |
| [vtium-config-core](https://vtium.github.io/vtium/api/vtium_config_core/) | The configuration that vtium-config's command line and TUI share: the drafts of vtium.conf and the operations on them, the store, the active resources, the installs, the flow, the font scan and the specimens, and the commands that use them. | the modules `drafts`, `ops`, `apply` and `flowcli` |
| [vtium-config-tab-settings](https://vtium.github.io/vtium/api/vtium_config_tab_settings/) | The Settings tab of vtium-config's TUI: vtium.conf's top-level keys in their sections, each key's value and whether it is changed, its allowed values in the list box, a typed value, its whole help, and the search by a key's name and description. | `Settings`, `Table`, `Values` |
| [vtium-config-macro](https://vtium.github.io/vtium/api/vtium_config_macro/) | Reads and writes vtium-config's macros: short text files of keys to press, text to type and checks of the screen, one step on each line. | `Macro`, `Step`, `Parser`, `parse` |
| [vtium-config-tab-help](https://vtium.github.io/vtium/api/vtium_config_tab_help/) | The Help tab of vtium-config's TUI: its topics at the left, and the keys of each tab with what each does at the right, from the TUI's compiled key table. | `HelpTab`, the modules `topics` and `page` |
| [vtium-config-tui](https://vtium.github.io/vtium/api/vtium_config_tui/) | The shell of vtium-config's TUI: the App that holds the six tabs as trait objects, the one navigation module, the update loop, the frame around the open tab, and the File and Actions menus. | `run`, `App`, `Nav`, `drive` |
| [vtium-clipboard](https://vtium.github.io/vtium/api/vtium_clipboard/) | The program `vtium-clipboard`, also installed as `vticlip`, which copies text from a script or a pipe to the console's clipboard through OSC 52. | `sequence`, `answers`, `run`, `Exit` |
| [vtium](https://vtium.github.io/vtium/api/vtium/) | The console program's logic: its configuration and command line, heads and consoles, the session's command and environment, and the on-screen display. | the modules `app`, `head` and `console` |
| [vtium-config-tab-flow](https://vtium.github.io/vtium/api/vtium_config_tab_flow/) | vtium-config's Flow tab: the configuration drawn as paths from the displays to the inputs, with NEW PATH and PARTS, on one page that scrolls. | `FlowTab`, the modules `page`, `paths`, `parts` and `inputs` |

The tools crates under `tools/` are vtium's own test harnesses, so the site does not publish their interfaces. The source of every crate is in [the crates folder on GitHub](https://github.com/vtium/vtium/tree/main/crates), with a README for each crate.
