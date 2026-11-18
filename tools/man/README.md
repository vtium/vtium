# vtium's man pages

`tools/man` holds vtium-man, the program that writes vtium's man pages in roff. The pages are committed in `man/` at the root of the repository, and `make install` installs them in `PREFIX/share/man`.

| Page | Where its facts come from |
|---|---|
| `vtium(8)` | The synopsis, the subcommands and their help come from the console's usage table, `notice::usage` in the vtium crate. The options that only the command line takes come from the key table. |
| `vtium-config(1)` | The commands come from vtium-config's help topics. The options come from `VALUE_OPTIONS`, the table its parser reads, and the exit statuses from `EXIT_STATUS`. |
| `vtium.conf(5)` | Every setting comes from the key table in vtium-settings, sorted into its sections. The sections of the file come from the text that vtium compiles in as its built-in sections. |
| `vtium-clipboard(1)` | The page comes from the help that `vtium-clipboard --help` prints. |
| `vtium-gallery(1)` | The page comes from the usage in the gallery script, `tools/gallery/gallery`. |

The prose that no table holds, such as the units, the kernel command line and the files, lives in the templates in `templates/`. Each page carries vtium's version, and the date of that version's row in `docs/releases/index.md`.

## Changing a page

Change the code or the template that holds the fact, then run `make man` and commit the pages it writes. `make man-check` fails when a committed page differs from what vtium-man writes. The check also runs `mandoc -T lint` on each page, and fails on a warning or worse.

A template is plain text. A blank line ends a paragraph, and each paragraph sits on one line. A few marks shape the page:

- `## TITLE` starts a section, and `### Title` starts a subsection.
- `= TERM` starts an item of a list of terms, and the lines below it are the item's paragraphs.
- `- text` is an item of a bulleted list.
- A line indented by four spaces is an example, shown as it is.
- `` `code` `` sets a command, an option, a setting or a file name in bold.
- `@NAME@` alone on a line is where vtium-man writes a block from code, such as `@SETTINGS@`.
