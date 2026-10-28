//! The committed specimens against a fresh drawing of them, so `cargo test`
//! fails on a change that moves an accepted profile until the moved files
//! are committed. A provisional profile's drift is printed and fails
//! nothing. No vtium-vitrify is given here, so the outline profiles are
//! skipped; `make specimens-check` draws them.

use std::path::Path;

use vtium_specimens::draw::Sources;

#[test]
fn every_accepted_specimen_matches_its_drawing() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/specimens");
    let sources = Sources {
        desktop: Err("cargo test builds no set; `make specimens-check` does".to_owned()),
    };
    let faults = vtium_specimens::check::check(&dir, &sources).unwrap();
    for d in &faults.drift {
        println!("drift in a provisional profile: {d}");
    }
    assert!(
        !faults.failed(),
        "specimens: {:?}\nsimulations: {:?}\n`make specimens` redraws them",
        faults.goldens,
        faults.simulations
    );
}
