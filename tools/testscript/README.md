# testscript

The tests of vtium-config, vtium and vtium-platform run stand-in programs they make themselves, such as a stand-in `vtium` or `systemctl`, or a copy of the test binary standing in for the image helper. `executable` writes a program's text to a file, and `executable_copy` copies a program, each with mode 0755. A test can run the program as soon as either function returns.

Linux refuses to run a file that any process holds open for writing, and fails with ETXTBSY ("Text file busy"). If a test wrote the file itself, a child process that another test thread starts during the write would inherit the open descriptor and hold it until the child runs its own program. So the test process never opens the file to write. A `sh` or `cp` of its own writes the file, and that process has exited before the function returns. `src/lib.rs` explains this in full.

Each crate whose tests use it names it as a dev-dependency, so no crate includes another crate's test file by path (vtium/vtium#339). `tests/busy.rs` runs 300 programs it writes and 300 it copies while four other threads start processes without pause, and checks that every one runs.
