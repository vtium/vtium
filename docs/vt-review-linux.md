# The Linux console's unverified rows

ADR-1000's tables have a `linux` column that records what the Linux kernel's own console does for each mode and feature: the console that the kernel draws through fbcon, which a program meets under `TERM=linux` on a VT where no other terminal runs. The column comes from console_codes(4), and from the kernel's source where that manual page is silent. A few rows depend on what the console shows on the screen or on what a key sends, and neither the page nor the source settles them, so they read `unverified`. This page lists them as steps for the maintainer's review on a real VT. An agent never runs these steps, because an agent on the development machine never opens a kernel console.

## Getting a kernel console

On this machine VTs 2 to 6 run vtium and VT 1 runs the display manager ([vt-review.md](vt-review.md#this-machines-vts)), so none of them shows the kernel's console. VT 8 has nothing on it. Start a login prompt there, which runs on the kernel's console, switch to it and log in:

```sh
sudo systemctl start getty@tty8.service
```

Press Ctrl-Alt-F8, log in, and check that `echo $TERM` prints `linux`. Step 4 also needs gpm, the daemon that lets the mouse select and paste on the kernel's console; start it with `sudo systemctl start gpm.service`. When the review is done, stop both with `sudo systemctl stop getty@tty8.service gpm.service`.

## The steps

For each step, type the command at the shell on VT 8, look at the result, and write down what you see beside the row ids. A row's cell in `docs/architecture/term/ADR-1000-features.tsv` then changes from `unverified` to what the step showed, in the words the other terminals' columns use: `yes`, `no` or `partial` with what is missing.

### 1. Blinking text: `sgr.blink`, `sgr.blink.style`, `sgr.blink.decorations`

```sh
printf 'plain \e[5mblinking\e[25m plain \e[4;5munderlined and blinking\e[0m plain\n'
```

Look at the word "blinking" for a few seconds.

- `sgr.blink`: does it blink? If it does not, say what SGR 5 shows instead, such as a brighter background or nothing at all.
- `sgr.blink.style`: if it blinks, does the text vanish in its off phase (`full only`), turn dim (`partial`), or something else?
- `sgr.blink.decorations`: the console draws an underline as a color on a color display, so the underline of "underlined and blinking" may show only as a different color. Does that color blink with the text, or stay?

### 2. A palette change recolors text already on the screen: `sgr.draw-time-color`

```sh
printf '\e[31mthis text is red\e[0m\n'; sleep 2; printf '\e]P1ffffff'; sleep 3; printf '\e]R'
```

`ESC ] P 1 ffffff` sets palette color 1, red, to white, and `ESC ] R` puts the palette back. The text is printed in red first. Does the line that is already on the screen turn white two seconds later, and red again three seconds after that? If it turns white and back, the row reads `yes`; if it stays red, `no`.

### 3. The bytes a Ctrl key sends: `input.legacy`

```sh
showkey -a
```

`showkey -a` prints the bytes of each key you press, and exits ten seconds after the last key. Press each key below and compare the bytes with the ones xterm sends, which the row asks for:

| Key | xterm sends |
|---|---|
| Ctrl-Space | 0 (`^@`) |
| Ctrl-2 | 0 (`^@`) |
| Ctrl-3 | 27 (`^[`) |
| Ctrl-4 | 28 (`^\`) |
| Ctrl-5 | 29 (`^]`) |
| Ctrl-6 | 30 (`^^`) |
| Ctrl-7 | 31 (`^_`) |
| Ctrl-8 | 127 (`^?`) |
| Ctrl-/ | 31 (`^_`) |
| Backspace | 127 (`^?`) |

If every key matches, the row reads `yes`. If some differ, it reads `partial` with the keys that differ.

### 4. Pasting with the mouse and with Shift-Insert: `clipboard.paste`

This step needs gpm running, as [Getting a kernel console](#getting-a-kernel-console) says.

```sh
cat
```

`cat` echoes what it reads, so a paste shows on the next line. Drag across a word on the screen with the left button to select it, then press the middle button. Does the word appear? Then press Shift-Insert. Does the word appear again? End `cat` with Ctrl-D. If both paste, the row reads `yes`; if only the middle button pastes, it reads `partial (the middle button; no Shift-Insert)`.
