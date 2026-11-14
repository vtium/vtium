# Real keyboard layouts for the keyboard tests

The files here are real keyboard layouts from [xkeyboard-config](https://gitlab.freedesktop.org/xkeyboard-config/xkeyboard-config), the layout data that X11, Wayland and vtium read through libxkbcommon. Each file is one layout as `xkbcli compile-keymap` compiles it, unchanged. The tests read the committed files, so a test sees the same layout on every machine, CI and the ARM runner included, whatever xkb data that machine has.

| File | Layout |
|---|---|
| `us.xkb` | US English, the control |
| `ru.xkb` | Russian |
| `de.xkb` | German, with its dead keys |
| `fr.xkb` | French AZERTY |
| `xkeyboard-config-COPYING` | xkeyboard-config's COPYING file, its copyright and license notices, unchanged |

## The rule

vtium's keyboard tests run against these real layouts, at least US, Russian, German and French AZERTY (ADR-1005). A change to how vtium handles keys adds or updates cases on these layouts, not only on US. A test that needs a layout fails when the file is missing or does not compile. It never skips.

Three test files read them. `crates/vtium-platform/src/input/tests.rs` checks the forms each key reports. `crates/vtium/tests/keyboard_layouts.rs` checks the bytes vtium-term's encoder sends for those keys. In `crates/vtium/tests/boot.rs`, `the_russian_layout_reaches_the_program` types on the Russian layout through the whole binary.

## How the files are made

`make keymaps` compiles every file again from this machine's xkeyboard-config and writes the versions it used below. It needs `xkbcli`, libxkbcommon's command line tool, which the package libxkbcommon provides on Arch, libxkbcommon-tools on Debian and libxkbcommon-utils on Fedora. For each layout `L` of the table, it runs this command, with the options spelled out so that no default of the machine applies:

```sh
xkbcli compile-keymap --output-format 1 --rules evdev --model pc105 --layout L --variant '' --options '' > L.xkb
```

`--output-format 1` writes the keymap text that every libxkbcommon reads, the one vtium links on older systems included.

Never edit a file here by hand. To change one, run `make keymaps` and commit the files and this README together. When the xkeyboard-config version changes, copy that version's COPYING file over `xkeyboard-config-COPYING` in the same commit, and update the version in vtium's COPYING notice.

<!-- keymaps version: written by make keymaps -->
The fixtures were compiled from xkeyboard-config 2.49 with xkbcli 1.13.2 (libxkbcommon).
<!-- end of keymaps version -->

## License

The layouts are xkeyboard-config's, under the notices of its COPYING file, which `xkeyboard-config-COPYING` holds. vtium's own COPYING names them among its third-party notices. vtium uses them in its tests only, and installs none of them.
