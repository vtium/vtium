---
contract: adr/v1
kind: decision
verb: add
capability: process
basis:
  - operator: aaronsb
    level: directed
    said: "I'd like to initially set up a way to package this. I think it could be a submodule. the goal is that we can package a local AUR package for now as a builder, and expand to other package managers eventually. This way I can essentially maintain the latest building copy and as we begin to actually publish version tags, I can keep it running on this machine."
    via: "the steering session, 2026-11-02, relayed in this record's brief"
  - operator: aaronsb
    level: directed
    said: "it can be it's own package repos in the vtium github org, and then submoduled in there"
    via: "the steering session, 2026-11-02, relayed in this record's brief"
  - operator: aaronsb
    level: directed
    said: "that initial packaging would be a real world test of profiles, deployment, updates, all the bits. I would expect it to lag behind our dev work of course, and if we're running on seperate vt sessions, they shouldn't collide."
    via: "the steering session, 2026-11-02, relayed in this record's brief"
  - operator: aaronsb
    level: directed
    said: "The recipes live in their own repos, submoduled into vtium; the vtium package lives alongside the installed kmscon package rather than replacing it; the -git package builds GitHub's main, with a setting to point it at a local checkout."
    via: "the maintainer's answers to the steering session's questions, 2026-11-02, relayed in this record's brief; the wording is the team lead's"
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "you know what, might as well just publish it on the real aur. I have rights there already"
    via: "the steering session, 2026-11-02, relayed by the team lead"
  - operator: aaronsb
    level: directed
    said: "if someone wants to install it that's their risk!"
    via: "the steering session, 2026-11-02, relayed by the team lead"
  - operator: aaronsb
    level: directed
    said: "I'd like to make sure we keep both vtium and vtium-git as ours"
    via: "the steering session, 2026-11-02, relayed by the team lead"
  - operator: aaronsb
    level: directed
    said: "let's make sure we gpg sign things too"
    via: "the steering session, 2026-11-02, relayed by the team lead"
  - operator: aaronsb
    level: directed
    said: "vtium-git is published on the real AUR under the maintainer's account from the vtium-arch repo, by a publish script that pushes each package directory to its AUR repo; until the staging make target is on main, the recipe does its install steps inside package(). vtium is published from a first pre-release, v0.1.0-alpha.1, tagged on main after this session's work merges there. Tags are signed with the maintainer's key 33F9B54FD2A4B6A4110749B3BA464FC9847A832A; the vtium recipe builds from the signed tag with that key in validpgpkeys and ships the public key under keys/pgp/; vtium-git stays unverified, because main's tip is a merge commit signed by GitHub's key; commits stay unsigned, because an agent's signing would ask for the passphrase each time."
    via: "the team lead's account of the maintainer's decisions, 2026-11-02; the wording is the team lead's"
    paraphrase: true
  - evidence: "measured 2026-11-01 on this machine: Arch's kmscon 10.1.0-1 from the extra repository owns /usr/bin/kmscon, /usr/lib/systemd/system/kmscon.service, /usr/lib/systemd/system/kmsconvt@.service and /etc/kmscon/kmscon.conf.example, and depends on kmscon-terminfo 10.1.0-1, which owns /usr/share/terminfo/k/kmscon; no package owns /etc/kmscon/kmscon.conf; /etc/systemd/system/autovt@.service links to kmsconvt@.service; getty.target wants getty@tty1 to getty@tty6; nothing is installed at /usr/bin/vtium, /usr/bin/vtium-vitrify, /usr/local/bin/vtium or /var/lib/vtium-vt-install; vtium/vtium-arch exists on GitHub and is empty"
  - evidence: "measured 2026-11-01: tic -x on crates/vtium/install/kmscon.ti, compared with infocmp against kmscon-terminfo's entry, gives vtium's entry XF, dim, sitm, ritm, smxx, rmxx, E3, BD, BE, PS, PE, Ms, Setulc, Smulx, Sync, XM, fd, fe, kxIN and kxOUT, which the installed entry lacks, kmous \\E[< where it has \\E[M, and setrgbf and setrgbb without its trailing ';'"
  - evidence: "measured 2026-11-01 with systemd 262's systemctl --root on a scratch tree holding kmsconvt@.service and a vtium@.service generated from the shipped units: with autovt@.service linked to kmsconvt@.service, `enable vtium@tty5.service` links autovt@tty5.service and getty.target.wants/vtium@tty5.service to vtium@.service; `enable vtium@.service` fails on the existing autovt@.service yet still links getty.target.wants/vtium@tty1.service from DefaultInstance=tty1, and with DefaultInstance= removed it links nothing; `disable vtium@tty5.service` leaves autovt@tty5.service; a second unit enabled on tty5 fails on autovt@tty5.service and still links its getty.target.wants entry"
  - evidence: "the tree on integration at 936ad31: the console runs its glyph helper from the path VTIUM_VITRIFY names at build time, else /usr/bin/vtium-vitrify (crates/vtium/src/helper.rs, default_program), and reads frits from VTIUM_FONT_FRITS, else /etc/vtium/fonts (crates/vtium/src/font.rs); the helper exits when idle and the next need starts it again, and the console gives a helper it cannot use up for the boot and draws with its built-in fonts (crates/vtium/src/helper.rs, module comment); the wire format starts with the magic VTVSREQ3 (crates/vtium-vitrify/src/serve.rs); `make install-tree` stages /usr/bin/vtium, /usr/bin/vtium-vitrify, kmsconvt@.service, kmsconvt-getty@.service, /usr/share/terminfo, /etc/kmscon/kmscon.conf.example and /usr/share/doc/vtium/examples (Makefile); `make vt-install` writes /usr/local/bin/vtium, /etc/systemd/system/vtium@.service and vtium-getty@.service and no helper (tools/vt/vt-install.sh); `vtium blank` and `vtium wake` find only a process named vtium (crates/vtium/src/blank.rs, named_vtium); the review runs hold /run/vtium-vt.lock and refuse a VT where vtium@ttyN is active or vt-install's manifest names it (tools/vt/run-vt.sh); the head file is /etc/vtium/heads.conf, fixed at build (crates/vtium-config/src/file.rs), and kmscon.conf's directory is the command-line key configdir, default /etc/kmscon (crates/vtium/src/config/keys.rs); vtium logs `vtium Revision <rev>` at start from git describe (crates/vtium/src/logger.rs, Makefile's minspec target)"
  - standard: "PKGBUILD(5), Using VCS Sources: a source entry is url#fragment, the url any URL git accepts prefixed with git+, and the fragment branch=, commit= or tag="
  - standard: "the Arch Wiki's VCS package guidelines: a package built from a version-control source carries the suffix -git, provides and conflicts with the package without it, and computes its version in a pkgver() function"
  - standard: "Arch's pacman hook 30-systemd-daemon-reload-system.hook: after a transaction that installs, upgrades or removes a file under usr/lib/systemd/system, pacman runs systemctl daemon-reload, and it restarts nothing"
  - precedent: ADR-1401
  - precedent: ADR-1301
agent:
  name: Claude
  model: claude-opus-5-5
observable:
  - "Installing vtium-git on this machine beside kmscon and kmscon-terminfo reports no conflicting file, and `pacman -Qo` on each file the package lists names vtium-git alone"
  - "`systemctl enable vtium@.service`, with no VT named, links nothing and leaves /etc/systemd/system/autovt@.service as it was"
  - "With vtium-git's vtium@tty5 running, `make vt-install VT=6` installs vtium-dev on VT 6, both run at once, `make vt-check` borrows a third VT, and `vtium blank --vt 6` blanks the development vtium"
  - "Upgrading vtium-git while vtium@tty5 runs leaves every session on VT 5 running, and the journal's `vtium Revision` line for VT 5 changes only after `systemctl restart vtium@tty5.service`"
  - "`makepkg` in `vtium-arch`'s `vtium` directory, with the maintainer's public key imported from `keys/pgp/`, builds `v0.1.0-alpha.1`, and refuses the build when the tag is unsigned or signed by another key"
  - "An AUR helper such as `yay`, asked for `vtium-git` on another Arch machine, installs the package the AUR carries, which matches `vtium-arch`'s `vtium-git` directory"
  - "A development build whose helper speaks a different wire format from the packaged helper still draws its frit's glyphs once installed with `make vt-install`"
status: proposed
date: 2026-11-01
deciders:
  - aaronsb
related: []
---

# ADR-1500: vtium is packaged from a recipe repo per package manager, and lives beside the C reference's package

## Summary

- **Decided:** vtium's packaging recipes live in one repository per package manager in the vtium GitHub organization, each a git submodule of vtium under `packaging/<manager>`. Arch comes first, in `vtium/vtium-arch`, published to the AUR under the maintainer's account: `vtium-git`, built from the tip of GitHub's `main` or, by a documented setting, from a local checkout and branch, and `vtium`, built from a signed version tag, the first being the pre-release `v0.1.0-alpha.1`. The package installs vtium beside the C reference's own Arch package, `kmscon`, and takes none of its files or unit names: it installs `/usr/bin/vtium`, the glyph helper `/usr/bin/vtium-vitrify`, and the units `vtium@.service` and `vtium-getty@.service`, and the user enables vtium on one VT at a time. A development install, `make vt-install`, moves to names of its own (`vtium-dev@.service`, `vtium-dev-getty@.service`, `/usr/local/bin/vtium-dev` and its own helper) so that the packaged vtium and a development vtium run on separate VTs at once. Both read the same configuration. An upgrade replaces the files and never restarts a running vtium; the new build runs from that VT's next start. Every problem the packaged vtium shows becomes an issue.
- **Trades away:** Packages that anyone on the AUR can install before vtium is ready for them, at their own risk. The development install's current names: `make vt-install` and the docs that name `vtium@ttyN` for it change. A second repository to keep, and a submodule pointer to move when it changes. The packaged vtium runs behind development on purpose. Until the maintainer answers the first open question, vtium's sessions under the package use the C reference's older terminfo entry.
- **One-way?** Expensive: the packages are published to the AUR, so other machines can install them by their names and enable `vtium@ttyN` by that name, and a rename strands them. A signed tag, once published, is never moved.
- **Probes:**
  - *Confident (upgrades):* an upgrade of the package never restarts vtium on a VT. The new build runs from that VT's next start: a reboot, a failure, or a restart the user asks for.
  - *Not confident (shared head file):* the development vtium and the packaged one read the same `/etc/vtium/heads.conf`, so a key that only the newer build knows breaks the file for the older one.
- **Inversion:** Between one install that the last build overwrites, as `make vt-install` over a package would, and a second machine for the packaged build. vtium keeps two installs on one machine under separate names, sharing the configuration.

## Context

The maintainer wants vtium installed on this machine the way a user installs it, through a package manager, and kept running there while development goes on. The package is a real test of what development alone never exercises: the maintainer's own head file and profiles in daily use, a package manager installing vtium's tree, upgrades under a running console, and removal. Arch is the package manager on this machine, and other package managers follow.

Two installs already compete for the names a package would take. `make vt-install` writes `/usr/local/bin/vtium` and the units `vtium@.service` and `vtium-getty@.service` into `/etc/systemd/system`, where a unit file overrides one of the same name in `/usr/lib/systemd/system`, the directory a package installs into. Left as it is, a development install would silently take over the packaged unit on every VT. And this machine runs the C reference's program as Arch ships it, the `kmscon` package (10.1.0) and its `kmscon-terminfo` package, which own `kmsconvt@.service`, `kmscon.service`, `/etc/kmscon/kmscon.conf.example`, `/usr/share/terminfo/k/kmscon` and, by its enablement, `/etc/systemd/system/autovt@.service`. `make install-tree`, the tree that tier 3 installs in its guest, stages vtium under several of those same paths, because vtium ships as a drop-in replacement for the C reference.

A VT (a Linux virtual console, switched with Ctrl-Alt-F<n>) runs one console at a time. logind starts `autovt@ttyN.service` on a VT that nothing holds when the user switches to it, and on this machine `autovt@` points at the C reference's `kmsconvt@`.

## Decision

### D1: one recipe repository per package manager, submoduled into vtium

- Each package manager's recipes live in their own repository in the vtium GitHub organization, and vtium holds each as a git submodule at `packaging/<manager>`. A submodule is a pointer from vtium's tree to one commit of the other repository, so vtium's history records which recipes go with which code, and the recipes keep their own history and can be published from their own repository.
- Arch comes first: `vtium/vtium-arch`, at `packaging/arch`. It holds `vtium-git`, which builds the latest `main`, and `vtium`, which builds a signed version tag (D2). Both names are vtium's on the AUR, which reserves no name, so each is published as soon as it can build: `vtium` from the first pre-release. Debian, Fedora and Nix each get a repository of their own later, named `vtium/vtium-<manager>` in the same way.
- A recipe stays thin: it calls vtium's own make targets to build and stage the files, so the knowledge of which file goes where lives in vtium's tree, and the package installs what tier 3 tests. vtium's Makefile gains a staging target for a package that lives beside the C reference's (D3), which every manager's recipe calls. Until that target reaches GitHub's `main`, the Arch recipe does the same install steps itself, inside its `package()` function, so that it builds from the `main` that exists today; it moves to the target once the target is on `main`.
- The Arch packages are published on the AUR, Arch's user repository, under the maintainer's account. `vtium-arch` holds one directory per package and a publish script that pushes each directory to that package's AUR repository, so the AUR always carries what `vtium-arch` holds. Anyone may install them, at their own risk. The maintainer builds them on this machine with `makepkg` the same way.

### D2: what the packages build

- `vtium-git` builds the tip of GitHub's `main`. Only reviewed work whose CI passed reaches `main`, so the packaged vtium is never a branch in progress.
- A documented setting in `vtium-arch` points the build at another git source and branch instead, such as a local checkout (`git+file:///path/to/vtium`, with `#branch=integration`), through makepkg's source entry (PKGBUILD(5)). makepkg clones that source, so the build takes the branch's last commit and never a checkout's uncommitted changes. The setting is read from the environment of the `makepkg` run, and `vtium-arch`'s README names it.
- The package follows Arch's rules for a package built from version control: it is named `vtium-git`, provides and conflicts with `vtium`, and computes its version in `pkgver()` from the commit it built, so `pacman -Q vtium-git` names the commit that the journal's `vtium Revision` line names at start.
- It builds the console with the features `make install-tree` builds by default, the minspec console, the one build tier 3 judges. The same setting may name features (`FEATURES='images logind'`).
- `vtium` builds one version tag, the first being the pre-release `v0.1.0-alpha.1`, tagged on `main` once the work this record starts has merged there. Its source entry names the tag and asks makepkg to check its signature (`git+https://github.com/vtium/vtium.git#tag=v0.1.0-alpha.1?signed`, PKGBUILD(5)), and its `validpgpkeys` holds the one key that signs vtium's tags, so makepkg refuses a tag that key did not sign. `vtium` conflicts with `vtium-git`, so a machine holds one of the two.
- **Signatures.** The maintainer signs each version tag with their key, `33F9B54FD2A4B6A4110749B3BA464FC9847A832A`. `vtium-arch` ships that key's public half under `keys/pgp/`, named for its fingerprint, so a builder can import it before building `vtium`. `vtium-git` checks no signature: the tip of `main` is a merge commit that GitHub signs with its own key, which says only that GitHub made the merge. Commits stay unsigned, because a delegated agent's signing would ask for the passphrase at every commit.

### D3: living beside the C reference's package

- The package installs `/usr/bin/vtium` and the glyph helper at `/usr/bin/vtium-vitrify`, the path the console runs it from when its build names none (`crates/vtium/src/helper.rs`). It installs `vtium@.service` and its getty fallback `vtium-getty@.service` into `/usr/lib/systemd/system`, generated from the shipped units in `crates/vtium/install/` by one generator in vtium's tree, which `make vt-install` uses too (D4). The generated `vtium@.service` is the shipped `kmsconvt@.service` with four changes: its name; `OnFailure=vtium-getty@%i.service`; `Conflicts=kmsconvt@%i.service` with `After=kmsconvt@%i.service`, so that starting vtium on a VT stops the C reference's console there first, as it stops a getty; and no `DefaultInstance=`, so that `systemctl enable vtium@.service` with no VT named fails and links nothing, where with `DefaultInstance=tty1` it would enable vtium on VT 1 while failing on `autovt@.service` (measured with systemd 262). `vtium-getty@.service` is the shipped `kmsconvt-getty@.service` under its new name.
- The package never ships `kmsconvt@.service`, `kmsconvt-getty@.service` or `kmscon.service`, and never takes `autovt@.service`, which stays the C reference's while its package is installed. The shipped unit's `Alias=autovt@.service` stays in the generated unit, so that enabling vtium on one VT links `autovt@ttyN.service` for that VT alone and logind starts vtium there, never the C reference's console.
- The package puts the example configuration at `/usr/share/doc/vtium/kmscon.conf.example`, beside the example udev rules and hwdb fragment, because the C reference's package owns `/etc/kmscon/kmscon.conf.example`. It ships no `/etc/kmscon/kmscon.conf` and no `/etc/vtium/heads.conf`: they are the user's.
- It ships no terminfo entry until the maintainer answers the first open question. Until then it depends on `kmscon-terminfo`, the C reference's entry, which a session in vtium finds under `TERM=kmscon`.
- The package enables and starts nothing: the VT to give vtium is the user's choice. The user turns vtium on for one VT that holds no session, from another VT or over ssh: `systemctl disable getty@ttyN.service`, then `systemctl enable vtium@ttyN.service`, then `systemctl start vtium@ttyN.service`, which stops the getty or the C reference's console on that VT through `Conflicts=`. `vtium-arch`'s README gives these steps and the steps back.
- Removing the package removes its files and changes no unit's state, so the user first runs `systemctl disable --now vtium@ttyN.service`, removes `/etc/systemd/system/autovt@ttyN.service`, which `disable` leaves (measured), and enables the getty again. The package's removal message names these steps, because a link left behind names a unit that no longer exists, and it is the link logind follows when it starts a console on that VT.
- Later, once the C reference's package is gone from the machines vtium is installed on, a variant of the package may install the shipped units under the names `kmsconvt@` and `kmsconvt-getty@`, the terminfo entry and the example under `/etc/kmscon`, as the drop-in replacement that `make install-tree` stages. That variant is not built.

### D4: the packaged vtium and a development vtium never collide

The packaged vtium owns the unit name `vtium@` and `/usr/bin/vtium`. `make vt-install` moves to names of its own, so the two run on separate VTs at once, and `make vt-check` and the other review runs borrow a third VT and touch neither. What each one owns, and what they share:

| What | The packaged vtium | The development vtium (`make vt-install`) | Shared, and why |
|---|---|---|---|
| The console binary | `/usr/bin/vtium` | `/usr/local/bin/vtium-dev` | Separate |
| The glyph helper | `/usr/bin/vtium-vitrify` | `/usr/local/bin/vtium-dev-vitrify`, with the development console built so that it runs that path | Separate. The console and its helper speak a wire format that changes between builds, so each console runs the helper built with it. Today `make vt-install` installs no helper, so a development console runs `/usr/bin/vtium-vitrify`, which would be the packaged, older helper |
| The units | `vtium@.service`, `vtium-getty@.service` in `/usr/lib/systemd/system` | `vtium-dev@.service`, `vtium-dev-getty@.service` in `/etc/systemd/system`, with `ExecStart=/usr/local/bin/vtium-dev`, `OnFailure=vtium-dev-getty@%i.service`, and `Conflicts=` with `After=` naming `vtium@%i.service` and `kmsconvt@%i.service` | Separate. A unit in `/etc/systemd/system` overrides one of the same name in `/usr/lib/systemd/system`, so a shared name would let the development unit replace the packaged one on every VT |
| A VT's `autovt@ttyN.service` link | Made by `systemctl enable vtium@ttyN` | Made by `make vt-install VT=N` | One link per VT, so one of them per VT. `systemctl enable` on a VT whose link names another unit fails, and still adds the unit to `getty.target`'s wants, so the user disables it again (measured); `make vt-install` refuses a VT where `vtium@ttyN` is enabled or the link names another unit, and the `Conflicts=` in the development unit stops the other on that VT if both are started |
| `/etc/kmscon/kmscon.conf` | Read | Read | Shared, with the C reference's console too. It is the user's configuration, and a key that only the newer build knows is logged and ignored by the older one |
| `/etc/vtium/heads.conf` and the frits in `/etc/vtium/fonts` | Read | Read | Shared: they hold the maintainer's heads and profiles, which the package exists to test in daily use. The second open question asks whether the development vtium should read a head file of its own |
| `/run/vtium/ttyN.closed` and `/run/vtium/ttyN/` | Written for its VT | Written for its VT | Shared directory, one file and one directory per VT. One vtium runs per VT, so each marker belongs to the vtium on that VT, and each getty fallback reads its own VT's marker (ADR-1401 D9) |
| `/run/vtium-vt.lock` | Never taken | Never taken | The review runs' alone: one review run at a time |
| `/var/lib/vtium-vt-install` | Never read or written | `make vt-install`'s record and manifests | `make vt-install`'s alone |
| The process name | `vtium` | `vtium-dev` | Separate. `vtium blank --vt N` and `vtium wake --vt N` look for a process named `vtium` today, and learn the name `vtium-dev` in the same work |

- `make vt-install` and `make vt-uninstall` move to the new names, and `make vt-install` installs the development helper with the binary, recorded and checked by the same checksums. vt-install refuses a VT where `vtium@ttyN` is enabled or running, as it refuses one where `kmsconvt@ttyN` runs.
- When the review runs pick a free VT, they pass over a VT where `vtium@ttyN` or `vtium-dev@ttyN` is enabled or running, and they refuse such a VT when it is named with `VT=N`, as they treat a VT that `make vt-install` holds today. `run-vt.sh` refuses a VT only while `vtium@ttyN` is running today, and an enabled vtium that is not running is the maintainer's console all the same.
- No VT on this machine holds a development install today (measured), so nothing installed under the old names needs moving. A machine that has one runs `make vt-uninstall VT=N` before it updates its checkout past this change.

### D5: updates

- The packaged vtium runs behind development on purpose. The maintainer rebuilds and upgrades it when they choose, and each upgrade exercises what a user's upgrade does: new files under a running console, a unit file changed under systemd, and the next start running the new build.
- An upgrade replaces the files and never restarts a running `vtium@ttyN`, because a restart hangs up every session on that VT. The package's install script restarts nothing, and Arch's systemd hook only reloads the unit files. A running vtium keeps the build it started with in memory, and the new build runs from that VT's next start: a reboot, a failure that its unit's `Restart=on-failure` starts again, or a restart the user asks for.
- To run the new build on purpose, the user logs out of the sessions on VT N and runs `systemctl restart vtium@ttyN.service` from another VT or over ssh; every session still on VT N ends. `journalctl -b -u vtium@ttyN.service | grep Revision` shows the build that VT runs, and `pacman -Q vtium-git` the build installed. The package's upgrade message names the restart, the sessions it ends, and the VTs whose `vtium@` is running.
- A helper with nothing to do exits after an idle interval, and the console starts it again at its next need for glyphs, so after an upgrade an older console can start the newer helper. When the wire format changed between the two builds, the console gives that helper up for the rest of its run and draws with its built-in fonts, as it does for any helper it cannot use (ADR-1301 D1), until vtium on that VT restarts. The third open question asks whether a console keeps the helper it started with.

### D6: what the package tests, and what happens to what it finds

- The packaged vtium tests the maintainer's head file and profiles in daily use, the staging target and the recipe as a package manager runs them, upgrades under a running console, the units beside the C reference's, a development vtium and the review runs on other VTs, and removal.
- Each problem it shows becomes an issue, chased like any other: on vtium/vtium when vtium, its units or its staging target is at fault, and on vtium/vtium-arch when the recipe is. The issue names the package's version and the revision the journal shows. It enters the plan's order, its fix carries a test that fails before the fix, and the packaged vtium takes the fix at its first build after the fix reaches `main`.

### For the operator's check

Each question below is open. The record's lean is written as its decision until the maintainer answers.

1. **Which terminfo entry named `kmscon` does this machine keep?** A terminfo entry tells programs such as `vim` and `htop` which features the terminal has; vtium and the C reference both use the name `kmscon`, and only one file can live at `/usr/share/terminfo/k/kmscon`. vtium's entry is the C reference's own entry at b1f7a7c5 with clipboard copy added, and it lists features that the installed entry, from release 10.1.0, lacks: italics, dim text, strikethrough, bracketed paste, focus reports, synchronized output, colored underlines, clipboard copy and the newer mouse format. For example, with the installed entry, `vim` in vtium shows italic text as plain text, because the entry does not say that vtium draws italics. The lean, written as D3: keep the C reference's entry while its package is installed, so its console never receives sequences it did not advertise, and ship vtium's entry in the later drop-in variant. The other way is a second package of vtium's entry that replaces `kmscon-terminfo`, which gives vtium's sessions every feature and gives the C reference's sessions an entry that promises features it may not have.
2. **Should the development vtium read a head file of its own?** For example, the maintainer tries a new head key in `/etc/vtium/heads.conf` for the development build; the packaged build, a few weeks older, does not know the key, so under `policy = open` it sets the file aside and uses its shipped rules, and under `policy = closed` it shows its panic screen on the daily console. `kmscon.conf` has no such risk, since an unknown key there is ignored. The lean, written as D4: share it, because the packaged vtium exists to test the maintainer's real file; and give the development vtium its own path at build time, as the frits' directory already has, once a head-file change first breaks the older build.
3. **Does a console keep the helper it started with?** For example, the maintainer upgrades the package on a Monday morning while vtium runs on VT 5; an hour later the frit's text on VT 5 turns into the built-in font, because the old console started the new helper and their wire formats differ. The lean: yes. The console opens the helper's program file at start and runs that same file each time it starts the helper, so an upgrade never mixes builds. That is a change to vtium's code, carried by its own issue; until it lands, D5's restart is the remedy.

## C notes: the C reference's packaging

- The C reference shipped two units, `scripts/systemd/kmsconvt@.service.in` and `scripts/systemd/kmscon.service.in` (kmscon b1f7a7c5), and its terminfo entry, `scripts/terminfo/kmscon.ti`. Its `kmsconvt@.service` carried `Alias=autovt@.service`, `WantedBy=getty.target` and `DefaultInstance=tty1`, and `OnFailure=getty@%i.service`.
- Arch's `kmscon` package installs those units under `/usr/lib/systemd/system` and splits the terminfo entry into its own package, `kmscon-terminfo`, on which `kmscon` depends. On this machine the entry is the one from release 10.1.0, which lacks features that the C reference's own entry at b1f7a7c5 lists; vtium's entry is that b1f7a7c5 entry with `Ms`, clipboard copy, added.

## Where vtium departs from the C reference

- **No default VT for the packaged unit.** The C reference's unit enabled VT 1 when enabled with no VT named. vtium's packaged unit enables nothing that way, so it can never start on the VT a desktop or another console holds by accident.

## Consequences

### Positive

- The maintainer runs a packaged vtium every day, so the head file, the profiles, the upgrades and the units are tested in the way users meet them.
- Development keeps its own install and its review runs while the packaged vtium runs, and neither replaces the other's files or units.
- The C reference's console keeps every file, name and default it had, so it stays usable on any other VT and the comparison with it stays honest.
- The recipes call vtium's own targets, so the files a package installs are the files tier 3 tests, and a new package manager adds a recipe and a repository, not a second account of vtium's layout.
- A user's sessions never end because of an upgrade.

### Negative

- `make vt-install`, `make vt-uninstall`, the review runs' VT checks, `vtium blank` and `vtium wake`, the VT review guide and docs/systemd.md change for the new names and the new checks, and the generator of the units moves out of `tools/vt/vt-install.sh` into a place both installs use.
- Until the first open question is answered, vtium's sessions under the package lack italics, dim text, bracketed paste and the other features vtium's own entry lists.
- After an upgrade, a VT can run an older build than the one installed until it restarts, and until the third open question's change lands, an older console can lose its frit to the built-in fonts.
- vtium has a second repository to keep, and a change to it reaches vtium's tree only when someone moves the submodule's pointer.
- Removing the package leaves the user to disable the unit and remove a link first, or the VT shows nothing when switched to.

### Neutral

- `packaging/` holds only submodules. A clone without `--recurse-submodules` has empty directories there, and no gate, CI job or build reads them.
- The recipes build against the same pinned sysroot and toolchain as `make install-tree`, so a build on this machine needs what `make install-tree` needs, and a network connection for the sysroot and the crates.
- Each later version tag moves `vtium`'s recipe to that tag, and the publish script pushes it to the AUR.

## Alternatives Considered

- **Recipes in vtium's own tree,** under `packaging/` as plain files. Rejected by the maintainer: a recipe repository of its own can be published and versioned apart from vtium, and the submodule still ties each recipe to the code it builds.
- **A package that replaces the C reference's,** providing and conflicting with `kmscon` and installing the drop-in tree that `make install-tree` stages. Rejected by the maintainer for now: the C reference's console stays installed on this machine beside vtium. D3 leaves it as a later variant.
- **A local builder only, published later.** Replaced by the maintainer: the packages go on the AUR now, since the maintainer holds an account there and the names stay vtium's.
- **Checking a signature on `vtium-git`'s source.** Rejected: the tip of `main` carries GitHub's signature on its merge commit, which proves nothing about vtium's maintainer.
- **Signed commits.** Rejected: an agent's every commit would ask for the passphrase; the signed tag is what a builder checks.
- **Building the local checkout by default.** Rejected by the maintainer: `main` holds only reviewed work with green CI, and the setting in D2 reaches a local checkout when one is wanted.
- **One set of names for both installs,** with `make vt-install` replacing the package's files. Rejected: the maintainer wants the packaged vtium kept running while development goes on, and a unit in `/etc/systemd/system` overrides the packaged one on every VT.
- **Keeping the development binary's name `vtium` in a directory of its own,** such as `/usr/local/lib/vtium-dev/vtium`, so that `vtium blank` finds it without change. Not chosen: `ps`, `pgrep` and the journal then show two processes with one name, and telling them apart is the point of separate names.
- **Restarting vtium after an upgrade,** as some packages restart their daemons. Rejected: a restart hangs up every session on the VT, so the user chooses when.
- **Enabling vtium on a VT from the package's install script.** Rejected: the VT to give vtium is the user's choice, and a VT given by mistake ends the session on it.
