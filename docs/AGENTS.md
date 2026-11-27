# AGENTS.md for docs/

This file covers work under `docs/`, and its [Writing](#writing) section sets how everything written for vtium reads, in every directory. The root [AGENTS.md](../AGENTS.md) still governs.

## Writing

- Write about vtium in the present tense everywhere you write: docs, comments, commits, PR bodies and issues. History can be told in the past tense.
- A decision record (an ADR, under `docs/architecture/`) never names a GitHub issue or PR by its number. The record says in a short phrase what the issue is about: what the work builds, what was found, or what was decided, as in "the work that draws right-to-left text". The record then carries its meaning on its own, the way a statically linked program carries its libraries, where a number only points somewhere else. The issue column of ADR-1000's TSVs is the one exception, because that column exists to name each row's issue. Issues, PRs, commits, code and the other docs keep using numbers.
- Say each fact once. Don't hard-wrap prose: one paragraph is one line in the file.
- Write the way an engineer explains something to a colleague out loud: in whole sentences, in plain and idiomatic words. For a technical reader, plain means the standard technical term, so a terminal "supports a protocol", "implements a feature" and "interprets a sequence". Don't be clever. Write so that someone who has never seen this code understands the text the first time they read it. Add the words a newcomer needs, and leave out the words that only restate what the reader already holds.
- Split a sentence that chains clauses and is hard to follow. This is a fix for a tangled sentence, not a mold for every sentence: two facts that belong together can share a sentence, joined by "and", "so" or "because". Write "vtium runs on a Linux virtual terminal (VT)—one of the text screens you can switch to with Ctrl+Alt and a function key. It takes the place of getty, the program that normally displays the login prompt.", not "vtium runs on a Linux virtual terminal (VT), one of the text screens that Ctrl+Alt and a function key switch to, in place of getty, the program that prints the login prompt there."
- Use the words readers already use for a thing, in pages people read: "renders", "images", "display server", "on supported hardware", not the project's own "draws", "pictures" or "where the machine has one". Where a standard technical term needs explaining, link a standard explanation and add a short gloss, as in "[KMS/DRM](https://en.wikipedia.org/wiki/Direct_Rendering_Manager), the modern graphics interface".
- Say what a thing does, and how. Mention something it lacks only where a reader would expect it, and then in a sentence of its own: "It can render entirely on the CPU, so a GPU is optional.", not "It draws with no display server and no GPU."
- Point to more detail at the end of a sentence, as "See [Provenance](#provenance) for details.", not in parentheses in the middle of one.
- Start an explanation from what the reader knows and move to what they don't, as VTium in depth starts from input and output on computers the reader has used before it reaches the VT. Give an example where a rule is abstract: "For example, one monitor could run a kiosk while another provides an operator's shell."
- Write a size with a multiplication sign (320×240, an 8×8 font), and put a comma before the last item of a list ("color emoji, 24-bit color, mouse reporting, and images").
- Unpack a compressed phrase into what it means. For example, "A frit of bitmap sources that states no cell takes its fonts' own cell" becomes "A frit made only of bitmap fonts, with no `cell=` line, draws at the fonts' own glyph size."
- A second example of unpacking: "Under the closed policy, a keyboard no rule mentions types nowhere, the guards are off, and no failure of vtium hands its VT to a getty's login prompt." becomes "Under the closed policy, vtium ignores any keyboard that no rule names, so its keys reach no session. The two guards that otherwise keep you from locking yourself out are off. If vtium fails, its VT is left with no login prompt." The first version packs three facts into one sentence, and "types nowhere" hides who acts. The second gives each fact its own sentence and names vtium as the one that acts.
- A sentence in a page people read stays under about 25 words. The limit is a ceiling that catches a packed sentence, not a length to aim for: a page reads well when short and longer sentences mix. `docs/scripts/prose` lists each sentence over 30 words in those pages, and each page whose sentences average over 20 words. Run `docs/scripts/prose --table` with the pages you changed, before and after you edit them. A sentence that must stay longer, for example one that quotes a long string, goes in `docs/scripts/prose-allow.tsv` with the reason the sentence stays. `docs/scripts/prose --check` is a gate. It fails on each long sentence that the allow file does not list, and on each page whose average is over 20 words.
- Define a project term the first time a section uses it, in the words a newcomer would use, as in "a frit (a font set file)". Prefer the everyday word where one exists.
- Name who does what when a sentence would otherwise hide it. Write "vtium ignores the keyboard", not "the keyboard falls to the policy's input default". This is a test for a hidden actor, not a rule that every sentence opens with vtium. Once a paragraph has said what vtium does, the next sentence can say "it", or make the thing itself the subject: "Each image is anchored to a line of text and scrolls with it."
- If you had to read a sentence twice, rewrite it before you commit. Then read the paragraph aloud. If it sounds like someone following a style guide rather than someone explaining the software, rewrite the paragraph.
- In an instruction that a reader follows one step at a time, name the thing again instead of writing "it", because the reader acts on each step alone. Write "Copy the file before you plant the defect, and afterwards restore the file from the copy.", not "Copy the file before you plant the defect, and afterwards restore it." Name the thing again too wherever "it" could mean two things: write "vtium passes the cluster to the font helper, which shapes the cluster.", not "vtium passes the cluster to the font helper, and it shapes it." Elsewhere, a pronoun reads naturally, as "What not to write" below says.
- When you brief a subagent that will write prose for people to read, put these rules in the brief so that the subagent follows them.
- Write the name as "vtium" in running text, code, paths and commands. Write "VTium" in a title or heading that names the project, with VT capitalized the way the hardware terminals printed it. Spell it "color", not "colour".

### What not to write

The rules above are tests for a problem in a sentence, not a template for every sentence. Applied to every sentence at once, they produce prose that is repetitive, overly literal and roundabout, where the effort to follow a style is louder than the person explaining the software. These are the signs of that, each with its fix.

- **Don't repeat a noun the reader already holds.** Use "it" or "they" when the thing was named in the same or the previous sentence and nothing else could be meant. Repeat the noun when two things could be meant, at the start of a paragraph, or in steps a reader follows one at a time. Not: "a program asks what kind of terminal the program talks to." Write: "a program asks what kind of terminal it talks to." A pronoun reads naturally inside a sentence, but plain English prose rarely opens a sentence with "it", so open the sentence with the thing or with who acts. Not: "It treats the file as broken." Write: "vtium treats the file as broken." The maintainer said this on 2026-11-17, about error messages that chained one "it" after another.
- **Don't swap a standard technical word for a plainer one.** Use the verb technical readers use: vtium supports a protocol, implements a feature, interprets a sequence. Not: "programs steer a terminal", "the features vtium takes in". Write: "programs control a terminal", "the features vtium implements".
- **Don't explain what the reader knows while leaving out what they don't.** Gloss a term the first time it appears, or leave it out of a page for users. Not: "A console has one screen and no window. So vtium moves and resizes no window." beside an unexplained "DRM dumb buffers". Write: "vtium fills the whole screen, so sequences that move or resize a window have nothing to act on.", and "memory buffers that the kernel's graphics driver displays".
- **Don't give every sentence the same shape.** Join facts that belong together and vary the length. Not: "vtium reads bitmap fonts. vtium reads outline fonts. Two fonts are built into vtium. So vtium starts without a font file." Write: "vtium reads bitmap and outline fonts, and two built-in fonts let it start without a font file."
- **Don't describe the page.** State the subject. Not: "Images names the protocols vtium supports." Write: "vtium shows images sent with sixel, the kitty graphics protocol, or iTerm2's inline images." A link at the end of a section can still say where to read more: "More in [Fonts](fonts.md)."
- **Don't labor a point a short phrase carries.** Not: "Each sequence does what the specification that defines the sequence says." Write: "Each sequence follows its defining specification."

## Pages for users and pages for contributors

The site's menu, in `zensical.toml`, sorts the pages. The pages under "For contributors" are for contributors: `architecture.md`, `vt-review.md`, `vt-review-linux.md` and `contributing/steering.md`. So are the decision records under `architecture/`, the acceptance passes under `acceptance/`, the design notes under `design-notes/`, and the pages on the crates' interfaces under `api/`. Every other page is for users.

The two kinds differ in two ways:

- In pages for people who use vtium (the README, the guides, the reference pages and the concepts pages), link the GitHub issue that tracks a feature or a gap, not the decision record behind that feature or gap: "See [the libseat issue](https://github.com/vtium/vtium/issues/34)." Decision records stay the links of pages for contributors: architecture.md, the VT review pages, the crates' READMEs and the records themselves. The reference pages for escape sequences under `docs/sequences/` are the one exception among pages for people who use vtium. Each one links both the issue that tracks its row and the decision record that governs the row, and says which link is which.
- In a guide, a tutorial, the README or an introduction, talk to the reader as "you" and say what they can do: "You can switch between VTs with Ctrl+Alt and a function key." A contributor page, a decision record, a spec and a code comment stay in the third person.

## Generated pages

Some pages are generated from the code. Never edit them by hand. Change their source, then run the command that writes them again.

| Page | Source | Command |
|---|---|---|
| `config.md` | `crates/vtium-settings/src/keys.rs` and `crates/vtium/src/config/doc.rs` | `VTIUM_BLESS=1 cargo test -p vtium generated_files` |
| `modes.md` and `modes.tsv` | `crates/vtium-term/src/terminal/modes.rs` | `VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents` |
| `specimens/` | the test card and its display profiles | `make specimens` |
| `showcase/` | tier 3's showcase screens | `make showcase` |

The tests behind the first two commands fail when a committed page differs from its source. `make specimens-check` draws the specimens again and compares them.

## A new page

Every new page needs a place in `zensical.toml`. Give the page an entry in the menu, `nav`, or a pattern in `not_in_nav` when another page links the new page instead. `cargo test -p vtium-site` fails on a page that has neither. `make docs` builds the site in strict mode, so it fails on a broken link or anchor.

## Figures

A figure lives in `figures/`, and `figures/README.md` has a row for the figure that says what the figure shows and how the figure is made. A figure is made in one of two ways:

- A hand-written SVG uses the vocabulary of the figures already there: their fonts, colors, line styles and legend. The SVG has a claim file beside the SVG, `figures/NAME.claims.tsv`, which lists each box, arrow, state and transition with the file and the symbol it rests on. Render the SVG with `rsvg-convert NAME.svg -o OUT.png` and look at the PNG at its natural size, on a light and on a dark background.
- A picture of vtium's own output is copied from a golden or drawn by vtium's renderer, by the command its README row gives, such as `make tui-shots` or tier 2's render. Never edit such a picture by hand. Draw the picture again.

A page whose sentences rest on the code can have a claim file too, such as `concepts/security.claims.tsv`.

## When the code changes

Run `docs/scripts/claims`. The script names each claim file and row whose file is gone, or whose symbol is no longer defined there. In the same change, update the page or the figure that row describes, and update the row too. When a figure changes, render the figure again and check the figure at its natural size.

A claim file names a symbol, never a line number. The symbol is a `fn`, a type, a `const` or a `mod`. The symbol can also be a key of the key table, written as `key NAME`, or a unit file's directive. `docs/scripts/claims --help` lists what each kind of symbol must match.
