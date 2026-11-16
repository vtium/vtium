<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 1337: iTerm2's set (SetUserVar, CurrentDir, File=, MultipartFile=, FilePart=, FileEnd)

| Sequence | What it does |
|---|---|
| `OSC 1337 ; Pt ST` | iTerm2's other strings, such as SetUserVar |

## Parameters

- `Pt`: A command's name, `=` and its argument, such as `SetUserVar=name=value`, where the value is in base64.

## What vtium does

vtium reads these strings and does nothing with them. The strings that carry pictures, `File=`, `MultipartFile=`, `FilePart=` and `FileEnd`, have a page of their own, image.iterm2.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** only iTerm2 and WezTerm document most of it; its pictures have a row of their own.
- **Note:** apart from File=, MultipartFile=, FilePart= and FileEnd: iTerm2's pictures, in one string or in parts, are decided in ADR-1103.
- **Specifications:** WezTerm es

## Tests

These tests check what vtium does with the sequences on this page.

- `the_osc_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
