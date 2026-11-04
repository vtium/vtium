# What VTium does not do

Some limits are deliberate, each recorded with its reason:

- **It never opens a link.** No browser or other program is ever started from the console; a link's address is only copied ([ADR-1002](../architecture/term/ADR-1002-links-are-copied-never-opened-and-prompt-marks-are-line-flags-the-view-jumps-between.md)).
- **It never hands the clipboard to a program.** A program's request to read the selection gets no answer, since the answer would go to whatever reads the terminal ([ADR-1000](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md), `osc.52.read`).
- **It never repeats a window title back.** A title query gets a fixed empty title, so a title a hostile program set cannot be replayed as input ([architecture.md](../architecture.md#determinism)).
- **It does not sandbox its unit.** The login session vtium starts would inherit every restriction the unit set ([ADR-1400](../architecture/platform/ADR-1400-the-privilege-and-sandbox-model.md)).
