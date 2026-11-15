<!-- tools/tier3/showcase.sh writes this page from tier 3's showcase run. Change the scenes in tools/tier3/guest/screens/ or the figures in docs/figures/ and run it again; an edit here is lost. -->

# Input and pointer

These screens are part of [vtium's showcase](README.md), which says how they are made.

## How it behaves

![Four numbered panels. 1: a program on a remote host sends OSC 52 with the word hello in base64. 2: vtium holds hello as its selection. 3: Shift+Insert or the middle button pastes hello into the shell. 4: a program sends OSC 52 with a question mark to read the selection, and vtium sends nothing back.](../figures/showcase-osc52-copy.svg)

A program copies text with OSC 52, even over SSH or inside tmux, where it shares no clipboard with you. vtium decodes the text and keeps it as its selection, the one a paste sends. Shift+Insert or the middle button pastes it into the console you are using. A program that asks to read the selection back gets no reply, so no program can read what you copied.

