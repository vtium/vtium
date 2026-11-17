<!-- tools/tier3/showcase.sh writes this page from tier 3's showcase run. Change the scenes in tools/tier3/guest/screens/ or the figures in docs/figures/ and run it again; an edit here is lost. -->

# Displays and VTs

These figures are part of [vtium's showcase](README.md), which says how they are made.

## How it behaves

![One head and its parts, inside vtium on one VT. On the left, a keyboard and a mouse on USB port 3.1 have solid arrows to the head desk, because a rule sends port 3.1 to desk. Below them, a roaming keyboard, which no head owns, has a dashed arrow to desk, which it types into while desk is the active head. The head desk comes from its [head desk] section. It holds a look, which is a font set and its size, and a stack of consoles. Console 1 is shown, with its own login, screen and scrollback, and consoles 2 and 3 run unseen behind it. Ctrl-Super-Right or Left shows another console. On the right, arrows go from desk to two displays, DP-1 and HDMI-A-1, because a rule sends each display to desk. Both displays show the console shown.](../figures/concepts-heads-parts.svg)

vtium drives every display of the machine from one VT, as a set of heads. A head is a terminal for one or more displays, with its own keyboards, its own look and its own consoles. Rules in vtium.conf send each display and each keyboard to a head, and vtium checks each device against the rules from the top. The first rule that matches decides where the device goes. Here one rule sends a keyboard and a mouse to the head desk, and another sends two displays to it.

![Two panels. In the first, one head, desk, shows on an 800×480 panel and on a 1920×1080 monitor. Both show a grid of 100 columns by 30 rows, the grid that the smaller display holds, and the larger display draws bigger characters. In the second, the head panel shows on the 800×480 panel with a grid of 100×30, and the head desk shows on the 1920×1080 monitor with a grid of 240×67. Each head has a grid sized for its own display, and its own sessions. A note says that the grids are counted in character cells for an 8×16 font.](../figures/concepts-heads-mirror.svg)

A head with several displays shows one grid on all of them, sized for the smallest display, and the larger display draws bigger characters. Two heads on the same two displays each have a grid sized for their own display, and sessions of their own.

![A timeline. The session keeps running in its pty the whole time. At first vtium draws on the display and receives keys and the pointer. The user switches away: vtium finishes its frame, gives up the display and stops reading input, and the other VT's owner draws and receives input. The user switches back: vtium takes the display, sets its modes and redraws the whole screen, and receives input again.](../figures/concepts-vt-handover.svg)

vtium shares the machine with a desktop or another console on a VT of its own. When you switch away, vtium finishes its frame, gives up the display and stops reading input. When you switch back, it takes the display again and redraws the whole screen. The programs in vtium's console keep running the whole time.

