# Configure vtium

This guide changes how vtium behaves through its configuration file, `/etc/vtium/vtium.conf`. vtium reads the keys the C reference's `kmscon.conf` took and adds its own; [config.md](config.md) lists every key with its default and its bounds.

The steps are, for now, in [managing-vtium.md](managing-vtium.md):

- [Writing vtium.conf](managing-vtium.md#configuration): starting from the example, the file's syntax, the same keys on the command line, when a change takes effect, and checking the configuration and the head file with `vtium check-config`.
- [Blanking from a script](managing-vtium.md#blanking-from-a-script): turning a VT's displays off and on again from outside its session.

The head file, `/etc/vtium/heads.conf`, decides which displays and keyboards make up each head, a terminal of its own for one or more displays. [Set up heads](heads.md) gives a complete head file for each common setup.
