# vtium's task runner. Each recipe is thin over cargo or a script in tools/;
# `make help` lists the targets. The VT review is described in
# docs/vt-review.md.

.DEFAULT_GOAL := help

# The binary the VT review runs. It runs as root, so it is named in this file
# or on make's command line (make vt-demo VTIUM_BIN=PATH), never taken from a
# stale export in the shell. The default is the minspec build, which the vt
# targets build first; another binary is run as it is.
VTIUM_MINSPEC := $(or $(CARGO_TARGET_DIR),target)/minspec/vtium
VT_GOALS := vt-demo vt-login vt-check vt-shell
ifneq ($(filter $(VT_GOALS),$(MAKECMDGOALS)),)
ifneq ($(filter environment%,$(origin VTIUM_BIN)),)
$(error VTIUM_BIN is set in the environment; the vt targets run it as root, so name it on make's command line: make $(firstword $(MAKECMDGOALS)) VTIUM_BIN=PATH)
endif
endif
# Simply expanded, so the recipes' $(value VTIUM_BIN) is the path itself.
ifeq ($(origin VTIUM_BIN),undefined)
VTIUM_BIN := $(VTIUM_MINSPEC)
endif
vt_build := $(if $(filter $(VTIUM_MINSPEC),$(VTIUM_BIN)),minspec)

.PHONY: help
help: ## Show this help
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z0-9_-]+:.*## / { printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

.PHONY: minspec
minspec: ## Build the minspec binary, target/minspec/vtium
	VTIUM_REVISION=$(or $(shell git describe --always --dirty 2>/dev/null),unknown) cargo build --profile minspec -p vtium

# vtium as a system installs it, staged under DESTDIR: the minspec binary,
# the VT unit, the compiled terminfo entry and the example configuration.
# Tier 3 installs this tree in its guest.
DESTDIR ?= target/install-tree
VTIUM_INSTALL := crates/vtium/install

.PHONY: install-tree
install-tree: minspec ## Stage vtium's install tree under DESTDIR (default target/install-tree; needs tic)
	install -Dm755 $(VTIUM_MINSPEC) $(DESTDIR)/usr/bin/vtium
	install -Dm644 $(VTIUM_INSTALL)/kmsconvt@.service $(DESTDIR)/usr/lib/systemd/system/kmsconvt@.service
	install -dm755 $(DESTDIR)/usr/share/terminfo
	tic -x -o $(DESTDIR)/usr/share/terminfo $(VTIUM_INSTALL)/kmscon.ti
	install -Dm644 $(VTIUM_INSTALL)/kmscon.conf.example $(DESTDIR)/etc/kmscon/kmscon.conf.example

# The ARM single-board computers minspec is judged against
# (docs/architecture.md): each target links against a pinned Debian sysroot
# and runs its tests under qemu-user, through tools/arm's wrappers
# (tools/arm/README.md). The sysroots live in a cache every checkout shares.
# The wrappers are named here, for these targets alone, so a native build on
# an ARM machine uses its own toolchain.
ARM_TARGETS      ?= aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf
ARM_TEST_TARGETS ?= aarch64-unknown-linux-gnu
ARM_WRAP         := $(CURDIR)/tools/arm
arm arm-test: export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = $(ARM_WRAP)/aarch64-unknown-linux-gnu-cc
arm arm-test: export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER = $(ARM_WRAP)/aarch64-unknown-linux-gnu-run
arm arm-test: export PKG_CONFIG_aarch64_unknown_linux_gnu = $(ARM_WRAP)/aarch64-unknown-linux-gnu-pkg-config
arm arm-test: export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER = $(ARM_WRAP)/armv7-unknown-linux-gnueabihf-cc
arm arm-test: export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_RUNNER = $(ARM_WRAP)/armv7-unknown-linux-gnueabihf-run
arm arm-test: export PKG_CONFIG_armv7_unknown_linux_gnueabihf = $(ARM_WRAP)/armv7-unknown-linux-gnueabihf-pkg-config

.PHONY: arm-sysroot
arm-sysroot: ## Fetch and verify the Debian sysroots the ARM targets link against (VTIUM_ARM_SYSROOTS=DIR)
	tools/arm/sysroot fetch arm64 armhf

.PHONY: arm
arm: arm-sysroot ## Build the minspec profile for each ARM target, target/TRIPLE/minspec (ARM_TARGETS='...'; needs clang)
	VTIUM_REVISION=$(or $(shell git describe --always --dirty 2>/dev/null),unknown) cargo build --profile minspec $(addprefix --target ,$(ARM_TARGETS))

.PHONY: arm-test
arm-test: arm-sysroot ## Run the workspace's tests for each ARM target under qemu-user (ARM_TEST_TARGETS='...'; needs clang, qemu-user)
	cargo test --workspace --all-features --no-fail-fast $(addprefix --target ,$(ARM_TEST_TARGETS))

# Tier 1 and the capability probes, against the goldens and probes in
# tools/conformance/, copied from the C reference (provenance.tsv).
.PHONY: conformance
conformance: ## Judge vtium against tier 1's goldens
	cargo build --release -p vtium-conformance --bins
	target/release/conformance run

.PHONY: conformance-probes
conformance-probes: ## Run the capability probes through vtium-replay
	cargo build --release -p vtium-conformance --bins
	target/release/conformance probes

# The parse oracle and the provenance check read libtsm and kmscon
# checkouts; neither is a gate.
LIBTSM ?= ../libtsm

.PHONY: conformance-provenance
conformance-provenance: ## Check each row of tools/conformance/provenance.tsv against its source with git show (LIBTSM=DIR KMSCON=DIR)
	cargo build --release -p vtium-conformance --bins
	target/release/conformance provenance --libtsm $(LIBTSM) --kmscon $(KMSCON)

.PHONY: parse-oracle
parse-oracle: ## Compare vtium-parse with the C reference's parser at tools/parse-oracle/pin.tsv (LIBTSM=DIR; needs gcc)
	LIBTSM=$(abspath $(LIBTSM)) cargo run --release --manifest-path tools/parse-oracle/Cargo.toml -- run

.PHONY: conformance-issues
conformance-issues: ## Check that every issue the conformance lists say owes a case or a capability is open (needs gh)
	cargo build --release -p vtium-conformance --bins
	target/release/conformance issues

.PHONY: test-tools
# shellcheck gates at warning: its info notes differ between versions, and
# the scripts' `A && B || die` is deliberate.
test-tools: ## Test the VT review tools and the tier 3 harness without a VT, sudo, logind or a VM (shellcheck when installed)
	python3 -I tools/vt/tests/test_vt.py
	python3 -I -B tools/tier3/test_t3img.py
	python3 -I -B tools/tier3/test_t3.py
	@if command -v shellcheck >/dev/null; then shellcheck -S warning -x tools/vt/*.sh tools/vt/checks/*.sh tools/vt/sections/*.sh tools/tier3/fetch-image.sh tools/tier3/guest/setup tools/tier3/guest/t3 tools/tier3/guest/tier3-install tools/arm/sysroot tools/arm/wrap && echo "shellcheck: clean at warning"; else echo "shellcheck: not installed, skipped"; fi

# The review targets block until the run ends, then print its answers. Their
# variables reach tools/vt/vt.sh through the environment, unexpanded, never as
# recipe text: VT=N picks the VT, DEMO_ARGS='...' passes vtium options (split
# like shell words, quotes honoured, nothing expanded), DRY_RUN=1 prints the
# commands without running them.
$(VT_GOALS): export VTIUM_VT_BIN = $(value VTIUM_BIN)
$(VT_GOALS): export VTIUM_VT_NR = $(value VT)
$(VT_GOALS): export VTIUM_VT_ARGS = $(value DEMO_ARGS)
$(VT_GOALS): export VTIUM_VT_DRY_RUN = $(value DRY_RUN)
# GUIDE and GUIDE_STEPS are make vt-check's; one exported in the shell does
# not reach the others.
$(VT_GOALS): export VTIUM_VT_GUIDE =
$(VT_GOALS): export VTIUM_VT_STEPS =

.PHONY: vt-demo
vt-demo: export VTIUM_VT_SESSION = demo
vt-demo: $(vt_build) ## Run vtium with the demo on a free VT, then print the answers (VT=N, DEMO_ARGS='...', DRY_RUN=1)
	tools/vt/vt.sh

.PHONY: vt-login
vt-login: export VTIUM_VT_SESSION = login
vt-login: $(vt_build) ## Run vtium with a login prompt on a free VT, until the first logout (VT=N, DEMO_ARGS='...')
	tools/vt/vt.sh

.PHONY: vt-shell
vt-shell: export VTIUM_VT_SESSION = shell
vt-shell: $(vt_build) ## Run your login shell, as you, in vtium on a free VT; exit it to end the run (VT=N, DEMO_ARGS='...')
	tools/vt/vt.sh

# Without GUIDE it fails before the build, naming the guides.
ifneq ($(filter vt-check,$(MAKECMDGOALS)),)
ifeq ($(GUIDE),)
$(error vt-check: GUIDE=NAME, a guide in tools/vt/checks: $(basename $(notdir $(wildcard tools/vt/checks/*.guide))))
endif
endif
.PHONY: vt-check
vt-check: export VTIUM_VT_SESSION = guide
vt-check: export VTIUM_VT_GUIDE = $(value GUIDE)
vt-check: export VTIUM_VT_STEPS = $(value GUIDE_STEPS)
vt-check: $(vt_build) ## Run a guide's checks on a free VT; the operator answers (GUIDE=NAME, GUIDE_STEPS='2 3')
	tools/vt/vt.sh

# Tier 2 against vtium's goldens and the C reference's, copied into
# tools/tier2/reference (provenance.tsv). tier2-provenance reads a kmscon
# checkout and is not a gate.
KMSCON ?= ../kmscon

.PHONY: tier2
tier2: ## Draw tier 2's screens and judge them against their goldens
	cargo build --release -p vtium-tier2 --bins
	target/release/tier2 run

.PHONY: tier2-provenance
tier2-provenance: ## Check tools/tier2's copies against their sources with git show (KMSCON=DIR)
	cargo build --release -p vtium-tier2 --bins
	target/release/tier2 provenance --kmscon $(KMSCON)

# Tier 3: vtium's install tree in a QEMU/KVM guest with a real kernel VT and
# KMS device, its screens captured and compared with the C reference's
# goldens at the commit tools/tier3/pin.tsv names, read from KMSCON
# (tools/tier3/README.md). QEMU runs headless as the invoking user. Each
# variant boots the machine tools/tier3/t3_scenario.py names; JOBS=N runs up to
# N variants at once, and with JOBS above 1, T3_DEADLINE=S stops a variant
# still running after S seconds. GL variants never run together.
T3_DIR      := tools/tier3
T3_VM       := target/tier3/vm
T3_PY       := python3 -I -B
JOBS        ?= 1
T3_DEADLINE ?= 1800
# The guest images: a cache every vtium checkout shares, which
# target/tier3/vm/images links to. A pristine or base image T3_REUSE already
# holds, the kmscon checkout's by default, is linked from there and only
# read; tier 3 writes nothing into another checkout.
T3_IMAGES   ?= $(or $(XDG_CACHE_HOME),$(HOME)/.cache)/vtium/tier3/images
T3_REUSE    ?= $(KMSCON)/fork/vm/images

# The pinned guest: the official Arch Linux cloud image, verified against the
# arch-boxes signing key, and the Arch Linux Archive snapshot its packages
# come from, so a rerun installs the same package versions.
T3_IMAGE_VER    := 20261015.611680
T3_IMAGE        := Arch-Linux-x86_64-cloudimg-$(T3_IMAGE_VER).qcow2
T3_IMAGE_URL    := https://geo.mirror.pkgbuild.com/images/v$(T3_IMAGE_VER)
T3_IMAGE_SHA256 := 08754e28d91b1680086c6211ef4df13559e766fff7cdb4006489b14b5d281f08
T3_IMAGE_KEY    := 1B9A16984A4E8CB448712D2AE0B78BF4326C6F8F
T3_ARCHIVE      := 2026/10/20

# What a report records it measured and ran on: this checkout's commit, with
# +dirty for uncommitted changes, and the guest's pins.
T3_REV = $(shell git rev-parse --short HEAD 2>/dev/null)$(if $(shell git status --porcelain --untracked-files=no 2>/dev/null),+dirty)
T3_RUN = T3_VTIUM_REV=$(T3_REV) T3_IMAGE=$(T3_IMAGE) T3_IMAGE_SHA256=$(T3_IMAGE_SHA256) \
	T3_ARCHIVE=$(T3_ARCHIVE) $(T3_PY) $(T3_DIR)/t3.py run --jobs $(JOBS) \
	--deadline $(T3_DEADLINE) --kmscon $(KMSCON)

.PHONY: tier3-image
tier3-image: ## Fetch, verify and provision tier 3's guest image, or reuse one built from the same inputs (T3_IMAGES=DIR, T3_REUSE=DIR)
	$(T3_PY) $(T3_DIR)/t3.py provision --image $(T3_IMAGE) --archive $(T3_ARCHIVE) \
		--url $(T3_IMAGE_URL) --sha256 $(T3_IMAGE_SHA256) --key $(T3_IMAGE_KEY) \
		--images $(T3_IMAGES) --reuse $(T3_REUSE)

# tier3 and tier3-minimal name the tree's target for t3.py, which builds it
# under the run lock, so a second run cannot rebuild the tree under a running
# one. The recipes hold no $(MAKE), so `make -n` runs nothing.
.PHONY: tier3
tier3: tier3-image ## Tier 3: build the install tree, then run every variant, or VARIANTS="..." (JOBS=N, KMSCON=DIR)
	$(T3_RUN) --tree-target install-tree $(VARIANTS)

.PHONY: tier3-minimal
tier3-minimal: tier3-image ## Tier 3 on the minspec machine, 1 vCPU and 512 MB: unifont, a PSF font, and the 320x240 and 640x480 displays (KMSCON=DIR)
	$(T3_RUN) --tree-target install-tree $(or $(VARIANTS),2d-potato-min 2d-potato-min-psf 2d-320x240-psf8 2d-640x480-psf16)

.PHONY: tier3-run
tier3-run: ## Tier 3 on a tree built already: the last run's, or TREE=DIR (VARIANTS=, JOBS= as for tier3)
	$(T3_RUN) $(if $(TREE),--tree $(TREE)) $(VARIANTS)
