<!-- tools/tier3/showcase.sh writes this page from tier 3's showcase run. Change the scenes in tools/tier3/guest/screens/ or the figures in docs/figures/ and run it again; an edit here is lost. -->

# Input and pointer

These screens and figures are part of [vtium's showcase](README.md), which says how they are made.

![A console screen in a smooth monospaced font: a heading, a list of four web addresses, and a line that names the ten pointer shapes vtium draws. The mouse pointer, a white hand with a raised finger and a dark outline, points at the second address, which has a dashed underline while the pointer is over it.](pointer.png)

A program changes the mouse pointer's shape with OSC 22, and vtium draws the shape itself. Here a program shows a list of links and sets the pointer to the hand that marks something to click. vtium has a picture of its own for each of ten shapes. They are the arrow, an I-beam over text, the hand, a crosshair, an hourglass, and the arrow with a small hourglass. The others are a circle with a slash, four arrows for a move, and double arrows for resizing sideways and up and down.

## How it behaves

![Four numbered panels. 1: a program on a remote host sends OSC 52 with the word hello in base64. 2: vtium holds hello as its selection. 3: Shift+Insert or the middle button pastes hello into the shell. 4: a program sends OSC 52 with a question mark to read the selection, and vtium sends nothing back.](../figures/showcase-osc52-copy.svg)

A program copies text with OSC 52, even over SSH or inside tmux, where it shares no clipboard with you. vtium decodes the text and keeps it as its selection, the one a paste sends. Shift+Insert or the middle button pastes it into the console you are using. A program that asks to read the selection back gets no reply, so no program can ask vtium for what you copied.

