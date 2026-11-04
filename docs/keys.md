# Keys

vtium reads the keyboard itself and passes every key to the session, apart from the keys it keeps for itself, such as those that switch VTs and scroll back. The table of those keys is, for now, in [managing-vtium.md](managing-vtium.md#usage), with the selection and the terminal type sessions get.

[The head file's terms](head-file.md#terms) define the keys that act on heads and consoles: the roaming chord, the console chords and the rescue key. [config.md](config.md) lists the keys that bind reboot, power off and suspend, `grab-reboot`, `grab-power-off` and `grab-suspend`, and `grab-paste`, which changes the paste keys; [systemd.md](systemd.md) says what each power key asks of the machine.
