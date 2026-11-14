# Heads: a terminal per display

This page describes how vtium drives several displays from one VT. Two parts of it are still to come. The first is a reload of vtium.conf that keeps the sessions running. [The reload issue](https://github.com/vtium/vtium/issues/86) tracks it. The second is a trial that applies an installed configuration and goes back to the previous one unless you confirm it. [The trial issue](https://github.com/vtium/vtium/issues/331) tracks it.

The kernel shows the same VT on every display. So vtium stays on its one VT, and it drives every display from one process, as a set of heads. A head is a terminal for one or more displays. Each head has its own inputs, its own look and its own consoles. A look is the font and the font settings a head draws with, and a console is a session of its own. For example, a small panel and a large monitor can each have a head with a grid sized for that display. Each display can also have a keyboard that types into its head.

The figure below shows one head, named desk, and what it is made of. A rule sends a keyboard and a mouse to desk, and another rule sends two displays to it. desk draws with its look, and it shows one of its consoles at a time.

![One head and its parts, inside vtium on one VT. On the left, a keyboard and a mouse on USB port 3.1 have solid arrows to the head desk, because a rule sends port 3.1 to desk. Below them, a roaming keyboard, which no head owns, has a dashed arrow to desk, which it types into while desk is the active head. The head desk comes from its [head desk] section. It holds a look, which is a font set and its size, and a stack of consoles. Console 1 is shown, with its own login, screen and scrollback, and consoles 2 and 3 run unseen behind it. Ctrl-Super-Right or Left shows another console. On the right, arrows go from desk to two displays, DP-1 and HDMI-A-1, because a rule sends each display to desk. Both displays show the console shown.](../figures/concepts-heads-parts.svg)

A head with several displays shows one grid on all of them. vtium sizes the grid for the smallest display, and the larger displays draw bigger characters. The figure below shows one head on two displays, beside two heads on the same two displays.

![Two panels. In the first, one head, desk, shows on an 800×480 panel and on a 1920×1080 monitor. Both show a grid of 100 columns by 30 rows, the grid that the smaller display holds, and the larger display draws bigger characters. In the second, the head panel shows on the 800×480 panel with a grid of 100×30, and the head desk shows on the 1920×1080 monitor with a grid of 240×67. Each head has a grid sized for its own display, and its own sessions. A note says that the grids are counted in character cells for an 8×16 font.](../figures/concepts-heads-mirror.svg)

You declare the heads in sections of vtium's configuration file, `/etc/vtium/vtium.conf`. The sections come below the file's settings. They declare three things: the heads, the look each head draws with, and an ordered list of rules. vtium reads the rules the way a firewall reads its rules. It checks each display and each keyboard or pointer against the rules from the top. The first rule that matches decides where the device goes. A rule can place the device in a head, take it out of use, or make it roam. A roaming keyboard types into the active head, and a key chord moves the active head from one head to another.

A rule names a device most firmly by its port, its serial number or its monitor's identity. A rule can also name a device by its name, which identifies it less firmly.

vtium ships rules that put every display in one head. They make every keyboard, pointer and other key device roam, media keys included. They tell these devices apart by the input classes that udev gives each device. They leave joysticks, tablets and similar devices out of use. With these rules, the console behaves as it does today. You add your own rules above them.

Like a firewall, the file has a default policy for the devices its rules don't mention. The policy is open or closed. See [Two policies: open and closed](#two-policies-open-and-closed) below.

A head can be isolated. When you press a key that switches the VT on one of an isolated head's keyboards, the key reaches the head's session as a plain key. No roaming keyboard ever types into an isolated head.

[heads.md](../heads.md) shows the sections in common setups, from the default to a kiosk and to several people working at once on one machine. Each setup has a complete file under [heads/](../heads/). [head-file.md](../head-file.md) is the reference for the sections. vtium-config, vtium's configuration tool, draws the same configuration as a flow from the displays to the input devices. It keeps your changes in a draft until you install it. See [Configure vtium](../configure.md).

When there is no vtium.conf, vtium runs its compiled-in configuration, and it writes no file. When the file has errors, vtium says so on the screen and in the log, and it leaves the file as it is. vtium runs the parts of the file that have no errors. When no part is left, it runs the shipped rules instead.

A file that declares `policy = closed` is the exception. When no part of a closed file is left, no keyboard types into anything until someone fixes the file from outside vtium.

Under the open policy, two guards keep you from locking yourself out, whatever the rules say:

- The anti-lockout guard makes every keyboard roam while no keyboard at all reaches a lit head. A head is lit when at least one of its displays is on.
- The rescue key is `Ctrl-Alt-F<n>` for the VT that is already on screen. It makes the keyboard that pressed it roam until the next boot.

Each head is a terminal of its own. Ctrl-Super-Up makes the next lit head the active head, and Ctrl-Super-Down makes the previous lit head the active head. Each head then shows a mark in its top-right corner for a few seconds. The mark is filled on the active head. See [The on-screen display and notices](osd-and-notices.md) for what vtium draws and how the `osd` setting changes it. A keyboard that a rule gives to a head keeps typing into that head when the active head moves.

The figure below shows three heads and the keyboards that type into each of them.

![Three keyboards on the left and three heads on the right. A roaming keyboard, which no head owns, has a dashed arrow to the head desk: it types into the active head. desk is the active head, and its mark is filled. A double arrow between desk and the head panel says that Ctrl-Super-Up or Down moves the active head, and panel can become the active head. A keyboard that a rule sends to panel has a solid arrow to panel, and it keeps typing into panel when the active head moves. A keyboard that a rule sends to the head kiosk has a solid arrow to kiosk. Its VT keys, such as Ctrl-Alt-F3, reach kiosk's session as plain keys and switch no VT. kiosk sets isolated = yes, so it is never the active head, and no roaming keyboard reaches it. The marks of panel and kiosk are hollow. A note says that each head shows the mark after the active head moves, filled on the active head and hollow on the others.](../figures/concepts-heads-keyboards.svg)

Ctrl-Super-Right shows the next console of the head that the keyboard types into, and Ctrl-Super-Left shows the previous one. A head starts with the number of consoles that its `consoles` key sets. A head can have at most 4 consoles, and all heads together at most 8, unless the configuration raises these limits. Under the open policy, Ctrl-Super-Return opens another console just after the one shown, within those limits. Ctrl-Super-Backspace closes the console shown, but it never closes a head's last console. Under the closed policy, a head keeps the consoles it started with, and neither key does anything.

With the `oneshot` setting, vtium exits only when no head has anything left to run. A head whose display never lit has nothing to run, so vtium never waits for it before it exits. See [Session ended](#session-ended-it-will-not-start-again) for what a console shows when its session ends under `oneshot`.

When the program in a console exits, vtium starts it again in the same console. Sometimes a login ends as soon as it starts, again and again. When that happens more than 5 times in a row, on any console of any head, vtium hangs up the sessions of every head. Under the open policy, vtium then exits with status 1, so a getty takes the VT. A getty is the program that normally shows the login prompt on a VT. Under the closed policy, vtium keeps its VT and shows a notice. See [All screens stopped](#all-screens-stopped-a-session-kept-ending-as-soon-as-it-started) for details.

When a head loses its last display, its `when-display-lost` key decides what happens to its sessions:

- `keep`, the default: the sessions keep running, unseen. The keys that reach the head still type into them, as keys typed on the kernel's console still arrive when its monitor is unplugged. A display that the head's rules match brings the sessions back.
- `hangup`: vtium hangs the sessions up at once, as a serial line hangs up when it loses its carrier.
- `hangup 30s`: vtium hangs the sessions up only when no such display returns within that time.

A mirrored head, one shown on several displays, keeps its sessions while one of its displays remains. The active head never moves because a display left or came back.

The figure below follows one head from the moment vtium reads it in vtium.conf to its end. Each rounded box is a state, and each arrow is the event that moves the head to the next state. A head's sessions can end or make vtium give up while the head is dark, as well as while it is running.

![A state machine of one head, inside a frame for while vtium runs. The head starts in waiting: vtium.conf declares the head, but no display of the head is set up yet. No session runs, and the head holds up no exit. When the VT is shown and a display of the head is set up, the head's consoles start, and the head enters a group of states in which its sessions run. In running, the head shows on its displays. One console is shown, and the others run unseen. Ctrl-Super-Right or Left shows another console. When a console's program exits, it starts again in that console. When the head's last display is disconnected, the head moves to dark. There, no display of the head is connected, the sessions run on unseen, and the keys that reach the head still type into them. A display that the head's rules match brings the head back to running. In dark, hangup, at once or when its time runs out, closes every pty, so the programs get SIGHUP, and each console starts again at its login. From either running or dark, under oneshot, the head moves to ended once every console's program has exited or a hang-up ended it. Each console then shows "Session ended. It will not start again." Once every head that started has ended, vtium exits with status 0. From any state, when a console's program on any head exits within 2 s of its start for the 6th time in a row, vtium gives up. The sessions of every head hang up. Under the open policy, vtium exits with status 1, and a getty takes the VT. Under the closed policy, vtium keeps the VT and shows a notice. A note says that each head moves through these states on its own, and that giving up stops every head at once.](../figures/concepts-heads-life.svg)

The notices that vtium shows in the ended and gave-up states are described under [When a session ends or cannot start](#when-a-session-ends-or-cannot-start).

If you prefer not to write the file by hand, vtium-config writes it for you. It runs on the console or in a desktop terminal, with or without root privileges. `vtium config flow` lists every connected display and input device, and the rule that places each one. You change your draft of the file in vtium-config's Flow view, or with `vtium config rule`, `vtium config head` and `vtium config look`. `vtium config install` checks the draft and installs it. See [Configure vtium](../configure.md).

Two features are not built yet. vtium-config cannot yet ask you to press a key to find out which keyboard belongs to which display. It also cannot yet report what the running console did with each device. See [the issue that tracks them](https://github.com/vtium/vtium/issues/367).

## When a session ends or cannot start

A head's consoles run their sessions. vtium shows a notice when a session ends for good, when a new console's session cannot start, or when sessions keep ending as soon as they start. See [Notices on the screen](../troubleshooting.md#notices-on-the-screen) for how long a notice stays.

### Session ended. It will not start again.

![vtium's notice over the last screen of a session, 800×480 pixels. A pale red box across the top of the screen holds one line: "Session ended. It will not start again. Press Ctrl+Super+↑ to switch to another screen." Below the box, the session's last screen stays as it was.](../figures/concepts-notice-oneshot.png)

**Why.** vtium.conf sets `oneshot`, or vtium was started with `--oneshot`. With `oneshot`, vtium does not start a console's program again when the program exits. The console keeps its last screen, and vtium shows this notice over it. The notice stays for as long as that console is shown.

**What to do.** The notice's second sentence names a key that takes you elsewhere. When no other console of the head still runs, it names the roaming chord, `Ctrl-Super-Up` by default, which makes another head the active head. When another console of the head still runs, it names the console chord, `Ctrl-Super-Right` by default, which shows that console. When vtium.conf binds no key to that action, the notice names no key.

**How to avoid it.** `oneshot` is off by default. Leave it off when you want vtium to start the program again each time the program exits. See [config.md](../config.md) for the key.

### Console not opened: login could not start.

![vtium's notice over a shell session, 800×480 pixels. A pale red box across the top of the screen holds one line: "Console not opened: login could not start." Below the box, the shell session goes on.](../figures/concepts-notice-no-console-opened.png)

**Why.** Under the open policy, `Ctrl-Super-Return` opens another console on a head. vtium shows this notice when it cannot start the new console's session. vtium then removes the new console, and the head goes on showing the console it showed before.

**What to do.** vtium's log gives the reason, in a line that begins `head NAME: console N does not open:`. Read it with `journalctl -b -u vtium@tty2.service` for VT 2. Fix what the line names, such as the command that the `login` key gives, then press `Ctrl-Super-Return` again.

**How to avoid it.** When you change the `login` key, run the new command by hand in a terminal first, to see that it starts.

### All screens stopped: a session kept ending as soon as it started.

![vtium's notice alone on the boot display, 800×480 pixels. A pale red box across the top of the screen holds the notice: "All screens stopped: a session kept ending as soon as it started. Nothing here takes input. Fix the login command in /etc/vtium/vtium.conf, then restart vtium or reboot." The rest of the screen is black.](../figures/concepts-notice-alone-stopped.png)

**Why.** A console's program exited within 2 seconds of its start more than 5 times in a row, so vtium gave up, as the figure of a head's life above shows. Most often, the command that the `login` key gives fails as soon as it starts. Under the closed policy, vtium shows this notice in place of exiting. vtium starts no session again, every display that a head used goes dark, and the boot display shows this notice alone. The boot display is the display that was on when vtium started, or else the first connected one. The notice stays for as long as vtium runs.

Nothing on this VT takes input. vtium delivers no key to anything, so no key switches to another VT either.

**What to do.** Fix the command that the `login` key gives from outside this console: over ssh, on a serial console, or from other boot media with the disk mounted. Then restart vtium, for example with `systemctl restart vtium@tty2.service`, or reboot. See [Recovery from outside](#recovery-from-outside) for details.

**How to avoid it.** Try a new `login` command under the open policy first. There, a login that keeps failing ends vtium, and a getty takes the VT, so you can still log in. Run the command by hand in a terminal before you put it in vtium.conf.

## Two policies: open and closed

vtium.conf's rules decide which keyboards reach which displays, much as a firewall decides which traffic passes. Like a firewall, the file has a default policy: `policy = open` or `policy = closed`. A file that does not set `policy` is open.

The rules mean the same under either policy. The policy decides the rest:

- what happens to an input that no rule matches,
- whether a head is isolated when the file does not set its `isolated` key,
- whether the rescue key and the anti-lockout guard are on, and
- what vtium does when it starts with no display.

A rule in the file does what it says under either policy. For example, in a closed file the rule `input MATCH -> roaming` sends whatever is plugged into that port to the active head. In a closed file, the active head is a head that sets `isolated = no`. This is how [kiosk-admin.conf](../heads.md#a-kiosk-with-an-operators-head) gives its admin head a spare port.

When a file's rules end with `input * -> TARGET`, that last rule places every input that no earlier rule placed. The policy then never decides where an input goes.

The figure below shows what each policy does with two keyboards. A rule sends the first keyboard to a head named kiosk, and no rule names the second.

![Two panels, policy = open and policy = closed. In both, a keyboard that a rule sends to kiosk has a solid arrow to the head kiosk. Under open, a keyboard that no rule names has a dashed arrow to the active head: it roams. A head is isolated only with isolated = yes, and the rescue key and the anti-lockout guard are on. Under closed, the arrow from a keyboard that no rule names ends in a cross: vtium ignores it and never opens it, so its keys reach no session. Every head is isolated unless it says isolated = no, and the rescue key and the anti-lockout guard are off.](../figures/concepts-heads-policy.svg)

**Open.** A keyboard or mouse that no rule matches roams. A head is not isolated unless it says `isolated = yes`. The rescue key and the anti-lockout guard are on.

Under the open policy, rules take rights away, as in "this keyboard belongs to the desk" or "the panel is isolated". So when you write a rule, you ask what you must stop from reaching everything. A forgotten rule lets input through. For example, a stranger plugs a keyboard into a spare port, and it types into someone's session.

The console as it is today is open. So is [builtin](../head-file.md#terms), the configuration vtium runs when vtium.conf has no sections.

**Closed.** vtium ignores any keyboard or mouse that no rule matches, so its input reaches no session. Every head is isolated unless it says `isolated = no`. The two guards that otherwise keep you from locking yourself out, the rescue key and the anti-lockout guard, are off. So vtium never moves a keyboard anywhere the file did not send it.

Under the closed policy, rules grant rights, as in "this keyboard belongs to the kiosk" or "the operator's head is not isolated". So when you write a rule, you ask what you must let in so that you don't shut yourself out. A forgotten rule locks you out: your own keyboard does nothing. A kiosk and a set of stations for several people are closed files.

If vtium fails under the closed policy, its VT is left with no login prompt. vtium can end because it panics, because its VT hangs up, or because it cannot start a session. After such a failure, vtium's systemd unit normally starts a getty on the VT as a fallback. Under a closed policy the fallback skips the getty, because the kernel's login prompt that a getty shows would take any keyboard.

The fallback belongs to vtium's unit alone, so the operator of a closed machine takes four more steps. See [systemd.md](../systemd.md) for details.

- The operator disables any `getty@ttyN` that is enabled on vtium's VT. A later `systemctl start` could still put that getty there.
- The operator masks the gettys on the other VTs. Once vtium has ended, the kernel acts on `Ctrl-Alt-F<n>` again, so the key could switch to one of those gettys.
- The operator sets logind's `NAutoVTs=0` and `ReserveVT=0`. Then logind starts no other vtium on a VT when someone switches to it.
- The operator sets a password on the boot loader. `vtium.enable=0` on the kernel command line turns vtium off for one boot. Under the closed policy, the fallback then skips the getty as well, so the key starts no login prompt. Other words on the kernel command line, such as `init=`, change what the machine runs at boot, and only a boot loader password keeps them out.

When a login keeps ending as soon as it starts, vtium stops every head and keeps its VT. See [All screens stopped](#all-screens-stopped-a-session-kept-ending-as-soon-as-it-started) for details.

Once vtium has read `policy = closed` in a file it trusts, nothing opens anything for as long as vtium runs. No other line of the file opens anything, whether that line is broken or not. Nothing that happens while vtium runs opens anything either.

Reload is not built yet. Once it is, these rules apply to it:

- While a closed policy runs, vtium takes only a valid closed file. For any other file, a valid open file included, vtium keeps running what it runs and logs why.
- To go from closed to open, you restart vtium. It then reads the file afresh, as it does at boot.
- While an open policy runs, a reload applies any valid file, a closed one included.

**With no display at start.** Under the open policy, vtium waits for the time that `display-wait` sets, 10 s by default, and then exits. Its unit, `vtium@ttyN`, then starts a getty on that VT. The getty shows the kernel's own login prompt, which any keyboard can type into. Under the closed policy, vtium keeps its VT and waits for a display however long it takes, so that getty never starts.

**A display that no rule places.** When vtium.conf is valid and closed, vtium leaves such a display blank, with or without kernel VTs. When vtium.conf is valid and open, such a display shows a notice that says why it has no terminal, such as `DP-9 is not configured for any terminal.` An open vtium.conf with no head and no rule gives every display the built-in default terminal instead. A display that a rule sends to `off` stays blank under either policy.

When vtium.conf has errors and its rules turn on no display at all, the display guard turns on the boot display. The boot display is the display that is on when vtium starts, or else the first connected one. See [Terms](../head-file.md#terms) for the exact order. In an open file, the boot display then shows the first head. In a closed file, it shows only the file's console line, or the panic screen.

### When no head has a display

A valid closed file can declare heads that all lack a display that is on. For example, the kiosk's monitor is unplugged at boot, and another display is connected. Or a rule names a monitor by its EDID serial, and the monitor was replaced. Every display then stays blank, and vtium shows no notice. This holds on every machine, with or without kernel VTs.

What a keyboard does then depends on its head:

- A head whose display has never been on receives no keys.
- A head whose last display went away keeps receiving keys from the keyboards that its rules give it. Its sessions run on unseen, as `when-display-lost` decides.
- The keyboards of an isolated head never switch VTs. Under the closed policy, every head is isolated unless it sets `isolated = no`.

**What to do.** Connect a display that a rule in vtium.conf sends to a head. When that display is on, its head shows on it. To change the rule instead, edit the file over ssh, then restart vtium.

**How to avoid it.** Run `vtium check-config` with the displays connected before you install a file. It checks that a connected display reaches a head, and it says what is missing when none does. Name a display by its port, with `connector` or `connector-path`, when the monitor on that port may change. A rule that names the monitor by `edid` stops matching when the monitor is replaced.

`vtium check-config` lists the keyboards, mice and displays present that no rule matches, and what the policy does with each one. It lists them as notes. For a closed file, it says prominently when no keyboard present can reach any head. You can run it before you install the file.

## The operator's way back in

### An operator's head

In a closed file, the operator can use a keyboard at the machine only through a head of their own. That head needs a display of its own, the setting `isolated = no`, and a rule that sends the operator's keyboard to it. [A kiosk with an operator's head](../heads.md#a-kiosk-with-an-operators-head) shows one.

A machine with one display has no display to spare for such a head. Its operator gets in from outside, or gives the one head `isolated = no`. Then `Ctrl-Alt-F2` on that head's keyboard switches to VT 2. There, a getty asks for a password before a passer-by can reach a shell. The head also becomes the active head, so any `-> roaming` rule in the file reaches it.

### Recovery from outside

When vtium.conf locks you out of a machine, you recover it the way you recover any misconfigured Linux system. You can log in over ssh or on a serial getty, or use out-of-band management. You can also boot other media and edit the file. vtium has no rescue boot of its own.
