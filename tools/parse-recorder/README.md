# parse-recorder

The recorder is a consumer of vtium-parse's actions. It records every action the parser gives it, and at each call it checks the bounds and ranges that vtium-parse's README promises: the number of parameters and sub-parameters, each value's range, the intermediates, the length of an OSC string, and how each string ends. A test feeds bytes through the parser into the recorder, then compares the recorded actions with the actions its specification gives.

vtium-parse's integration tests in `crates/vtium-parse/tests/` and its fuzz target in `crates/vtium-parse/fuzz/` use it. Each names this crate as a dependency, so neither includes the other's file by path (vtium/vtium#339). This crate depends on vtium-parse, and vtium-parse names it as a dev-dependency, which cargo allows.
