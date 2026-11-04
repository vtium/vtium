# vtium's task runner. Each recipe is thin over cargo or a script in tools/;
# `make help` lists the targets. The VT review is described in
# docs/vt-review.md.

.DEFAULT_GOAL := help

# The x86_64 build links against the bookworm sysroot (below), so the binaries
# a release ships need at most glibc 2.36.
X86_TARGET := x86_64-unknown-linux-gnu

# The binary the VT review runs. It runs as root, so it is named in this file
# or on make's command line (make vt-accept VTIUM_BIN=PATH), never taken from a
# stale export in the shell. The default is the minspec build, which the vt
# targets build first; another binary is run as it is. make vt-install's
# default is the development console, the minspec build made to start its
# glyph helper at VTIUM_DEV_HELPER, in a target directory of its own so that
# it never replaces the minspec build. The helper, VTIUM_CONFIG_BIN, is the
# release build of vtium-config unless named: make vt-install installs it,
# and the review runs copy it into /run/vtium-vt/ and run vtium with
# --font-helper naming the copy, so each run starts the helper built with it.
# With images in FEATURES, make vt-install's console also starts its image
# helper at VTIUM_DEV_IMAGE, and the helper make vt-install installs there,
# VTIUM_IMAGE_BIN, is the release build of vtium-image unless named.
VTIUM_MINSPEC := $(or $(CARGO_TARGET_DIR),target)/$(X86_TARGET)/minspec/vtium
VTIUM_DEV_DIR := $(or $(CARGO_TARGET_DIR),target)/vt-dev
VTIUM_DEV := $(VTIUM_DEV_DIR)/$(X86_TARGET)/minspec/vtium
VTIUM_DEV_HELPER := /usr/local/bin/vtium-dev-config
VTIUM_DEV_IMAGE := /usr/local/bin/vtium-dev-image
# The line only a build with images carries, by which make vt-install tells a
# console with images: tools/vt/vt-install.sh names it.
VT_IMAGES_MARKER := $(shell sed -n "s/^images_marker='\(.*\)'$$/\1/p" tools/vt/vt-install.sh)
CONFIG_RELEASE := $(or $(CARGO_TARGET_DIR),target)/$(X86_TARGET)/release/vtium-config
IMAGE_RELEASE := $(or $(CARGO_TARGET_DIR),target)/$(X86_TARGET)/release/vtium-image
VT_GOALS := vt-accept vt-login vt-check vt-shell
VT_INSTALL_GOALS := vt-install vt-uninstall
ifneq ($(filter $(VT_GOALS) $(VT_INSTALL_GOALS),$(MAKECMDGOALS)),)
ifneq ($(filter environment%,$(origin VTIUM_BIN)),)
$(error VTIUM_BIN is set in the environment; the vt targets run it as root, so name it on make's command line: make $(firstword $(MAKECMDGOALS)) VTIUM_BIN=PATH)
endif
ifneq ($(filter environment%,$(origin VTIUM_CONFIG_BIN)),)
$(error VTIUM_CONFIG_BIN is set in the environment; the vt targets give it to vtium, which starts it from root, so name it on make's command line: make $(firstword $(MAKECMDGOALS)) VTIUM_CONFIG_BIN=PATH)
endif
ifneq ($(filter environment%,$(origin VTIUM_IMAGE_BIN)),)
$(error VTIUM_IMAGE_BIN is set in the environment; make vt-install installs it for vtium, which starts it from root, so name it on make's command line: make $(firstword $(MAKECMDGOALS)) VTIUM_IMAGE_BIN=PATH)
endif
endif
# Simply expanded, so the recipes' $(value VTIUM_BIN) is the path itself.
ifeq ($(origin VTIUM_BIN),undefined)
VTIUM_BIN := $(if $(filter vt-install,$(MAKECMDGOALS)),$(VTIUM_DEV),$(VTIUM_MINSPEC))
endif
ifeq ($(origin VTIUM_CONFIG_BIN),undefined)
VTIUM_CONFIG_BIN := $(CONFIG_RELEASE)
endif
ifeq ($(origin VTIUM_IMAGE_BIN),undefined)
VTIUM_IMAGE_BIN := $(IMAGE_RELEASE)
endif
vt_build := $(if $(filter $(VTIUM_MINSPEC),$(VTIUM_BIN)),minspec)
vt_helper_build := $(if $(filter $(CONFIG_RELEASE),$(VTIUM_CONFIG_BIN)),config-release)
# These two read IMAGES, from FEATURES, below, so they expand where they are
# used.
vt_image_build = $(if $(and $(IMAGES),$(filter $(IMAGE_RELEASE),$(VTIUM_IMAGE_BIN))),image-release)
vt_dev_build = $(if $(filter $(VTIUM_DEV),$(VTIUM_BIN)),vt-dev-console) $(vt_helper_build) $(vt_image_build)

.PHONY: help
help: ## Show this help
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z0-9_-]+:.*## / { printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

# The minspec console binary links with packed relative relocations
# (DT_RELR), which saves 65,848 B (vtium/vtium#143's floor: glibc 2.36 reads
# them; the linker needs binutils 2.38 or lld 15). cargo rustc hands the flag
# to the vtium binary's link alone, so the default dev build, cargo test and
# the other crates' links stay as they are.
RELR_LINK := -C link-arg=-Wl,-z,pack-relative-relocs

# check-relr FILE fails unless the dynamic section of FILE carries DT_RELR, so
# the flag cannot drop silently.
define check-relr
readelf -d $(1) | grep -q '(RELR)' || { echo "$(1) carries no DT_RELR: the minspec link lost -z pack-relative-relocs" >&2; exit 1; }
endef

# vtium/vtium#143: a shipped binary needs at most glibc GLIBC_FLOOR, the
# release Debian 12 carries. check-glibc FILE reads the highest GLIBC_ version
# FILE requires and fails above the floor.
GLIBC_FLOOR := 2.36
define check-glibc
top=$$(readelf -V $(1) | grep -o 'GLIBC_[0-9][0-9.]*' | sort -Vu | tail -n1); \
echo "$(1): highest required $${top:-no GLIBC_ version}"; \
[ "$$(printf '%s\nGLIBC_$(GLIBC_FLOOR)\n' "$$top" | sort -V | tail -n1)" = GLIBC_$(GLIBC_FLOOR) ] || { echo "$(1) needs $$top, over the floor GLIBC_$(GLIBC_FLOOR)" >&2; exit 1; }
endef

# The x86_64 builds link through tools/arm's wrappers against the bookworm
# sysroot, as the ARM targets do; the wrappers are named for these targets
# alone, so the default build and cargo test use the host's toolchain.
ARM_WRAP := $(CURDIR)/tools/arm
minspec install-tree vt-dev-console config-release image-release: export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = $(ARM_WRAP)/$(X86_TARGET)-cc
minspec install-tree vt-dev-console config-release image-release: export PKG_CONFIG_x86_64_unknown_linux_gnu = $(ARM_WRAP)/$(X86_TARGET)-pkg-config

# The vtium crate's features the console binary is built with, named on
# make's command line (make install-tree FEATURES='images logind'). None, the
# default, builds the minspec console, the only binary tier 3's budget judges
# (ADR-1200 D19). The assignment here outranks the environment, so a
# FEATURES exported in the shell builds nothing different.
FEATURES :=
# The acceptance pass shows pictures by default (ADR-1501 D5): make vt-accept
# builds the console with images unless make's command line names FEATURES,
# so make vt-accept FEATURES= is a pass of the minspec build. A binary named
# with VTIUM_BIN is run as it is, so its features are the ones it was built
# with: images when it carries the line only a build with images carries
# (VT_IMAGES_MARKER), else none, and the run gives it the image helper only
# when it has pictures.
ifneq ($(filter vt-accept,$(MAKECMDGOALS)),)
ifneq ($(origin FEATURES),command line)
ifeq ($(filter $(VTIUM_MINSPEC),$(VTIUM_BIN)),)
FEATURES := $(if $(shell grep -qaF -- '$(VT_IMAGES_MARKER)' '$(VTIUM_BIN)' 2>/dev/null && echo images),images)
else
FEATURES := images
endif
endif
endif

.PHONY: minspec
minspec: ## Build the minspec binary for x86_64 against the glibc 2.36 sysroot, target/x86_64-unknown-linux-gnu/minspec/vtium, with FEATURES='...' when named (needs clang)
	tools/arm/sysroot fetch amd64
	VTIUM_REVISION=$(or $(shell git describe --always --dirty 2>/dev/null),unknown) cargo rustc --profile minspec -p vtium --bin vtium --target $(X86_TARGET) $(if $(strip $(FEATURES)),--features '$(strip $(FEATURES))') -- $(RELR_LINK)
	$(call check-relr,$(VTIUM_MINSPEC))
	$(call check-glibc,$(VTIUM_MINSPEC))

# vtium as a system installs it, staged under DESTDIR: the console binary,
# built with the minspec profile and FEATURES, vtium-config, with images in
# FEATURES the image helper vtium-image and its sysusers file, which creates
# its account for the login prompt (ADR-1101 D1, D2), the VT unit
# vtium@.service and its getty fallback vtium-getty@.service, generated by
# tools/vt/units.sh as make install generates them (ADR-1500 D3), the
# compiled terminfo entry, the example configuration
# /etc/vtium/vtium.conf.example beside the file vtium reads,
# /etc/vtium/vtium.conf (ADR-1500 D3, D4), and the example udev rules and
# hwdb fragment, which are documentation and never active (ADR-1401 D4).
# Tier 3 installs this tree in its guest.
# DESTDIR's default is install-tree's alone: make install refuses it below.
DESTDIR ?= target/install-tree
VTIUM_INSTALL := crates/vtium/install
# The image helper ships only with a console built with images: a minspec
# build installs none (ADR-1101 D1). FEATURES may separate its features
# with commas or spaces, as cargo's --features does.
comma := ,
IMAGES := $(filter images,$(subst $(comma), ,$(FEATURES)))

.PHONY: config-release
config-release: ## Build vtium-config, the font tool and glyph helper, for x86_64 against the glibc 2.36 sysroot, target/x86_64-unknown-linux-gnu/release/vtium-config
	cargo build --release -p vtium-config --target $(X86_TARGET)
	$(call check-glibc,$(CONFIG_RELEASE))

.PHONY: image-release
image-release: ## Build vtium-image, the image helper, for x86_64 against the glibc 2.36 sysroot, target/x86_64-unknown-linux-gnu/release/vtium-image
	cargo build --release -p vtium-image-helper --target $(X86_TARGET)
	$(call check-glibc,$(IMAGE_RELEASE))

.PHONY: install-tree
install-tree: minspec config-release $(if $(IMAGES),image-release) ## Stage vtium's install tree under DESTDIR (default target/install-tree), its console built with FEATURES='...' when named (needs tic)
	install -Dm755 $(VTIUM_MINSPEC) $(DESTDIR)/usr/bin/vtium
	$(call check-relr,$(DESTDIR)/usr/bin/vtium)
	$(call check-glibc,$(DESTDIR)/usr/bin/vtium)
	install -Dm755 $(CONFIG_RELEASE) $(DESTDIR)/usr/bin/vtium-config
	$(call check-glibc,$(DESTDIR)/usr/bin/vtium-config)
	tools/vt/units.sh --name vtium --exec /usr/bin/vtium --by 'make install-tree' --conflicts 'kmsconvt@%i.service' $(DESTDIR)/usr/lib/systemd/system
	install -dm755 $(DESTDIR)/usr/share/terminfo
	tic -x -o $(DESTDIR)/usr/share/terminfo $(VTIUM_INSTALL)/xterm-vtium.ti
	tic -x -o $(DESTDIR)/usr/share/terminfo $(VTIUM_INSTALL)/kmscon.ti
	install -Dm644 $(VTIUM_INSTALL)/vtium.conf.example $(DESTDIR)/etc/vtium/vtium.conf.example
	install -Dm644 -t $(DESTDIR)/usr/share/doc/vtium/examples $(VTIUM_INSTALL)/examples/71-vtium-local.rules $(VTIUM_INSTALL)/examples/vtium.hwdb
ifneq ($(IMAGES),)
	install -Dm755 $(IMAGE_RELEASE) $(DESTDIR)/usr/bin/vtium-image
	$(call check-glibc,$(DESTDIR)/usr/bin/vtium-image)
	install -Dm644 $(VTIUM_INSTALL)/vtium-image.conf $(DESTDIR)/usr/lib/sysusers.d/vtium-image.conf
endif

# vtium as a package ships it, staged under DESTDIR with PREFIX, so that a
# package's build can be one call: make install DESTDIR="$pkgdir"
# PREFIX=/usr PKGNAME="$pkgname" (ADR-1500 D1, D3). packaging/arch's AUR
# recipes call this target and install-terminfo. make install stages the
# console, built with the minspec profile and FEATURES and made to start its
# glyph helper at PREFIX/bin/vtium-config, the helper, with images in
# FEATURES the image helper, which the console starts at
# PREFIX/bin/vtium-image, and its sysusers file in PREFIX/lib/sysusers.d,
# which creates the helper's account for the login prompt (ADR-1101 D1,
# D2), the VT unit
# vtium@.service and its getty fallback vtium-getty@.service, generated by
# tools/vt/units.sh, the example configuration
# /etc/vtium/vtium.conf.example, which stays in /etc whatever PREFIX is,
# because vtium reads its configuration from /etc/vtium alone (ADR-1500 D3,
# D4), the example udev rules and hwdb fragment as documentation, and the
# license under PKGNAME's name. make install-terminfo stages vtium's
# terminfo entry, compiled under its name xterm-vtium, for the package that
# holds it alone. Neither stages a file the C reference's packages own:
# nothing under /etc/kmscon, no terminfo entry named kmscon and no kmsconvt@
# unit. The builds use the host's toolchain, as a distribution's package
# builds do, in a target directory of their own, INSTALL_TARGET_DIR.
# CARGO_BUILD_FLAGS goes to each cargo call, as --frozen for a package build
# that fetched its crates first.
PREFIX := /usr/local
PKGNAME := vtium
INSTALL_TARGET_DIR := $(or $(CARGO_TARGET_DIR),target)/install
CARGO_BUILD_FLAGS :=
UNITDIR := $(PREFIX)/lib/systemd/system
TERMINFO_SRC := $(VTIUM_INSTALL)/xterm-vtium.ti
# Both stage under DESTDIR alone: DESTDIR is named, not empty, and not the
# machine's own root, so neither writes a file over the packaged vtium's or
# beside it in /usr/local.
ifneq ($(filter install install-terminfo,$(MAKECMDGOALS)),)
ifeq ($(origin DESTDIR),file)
$(error $(firstword $(filter install install-terminfo,$(MAKECMDGOALS))): name DESTDIR=DIR, the directory it stages the files under; it never installs onto this machine)
endif
ifeq ($(strip $(DESTDIR)),)
$(error $(firstword $(filter install install-terminfo,$(MAKECMDGOALS))): DESTDIR is empty, which would install onto this machine; name the directory it stages the files under)
endif
ifeq ($(shell readlink -m -- '$(DESTDIR)'),/)
$(error $(firstword $(filter install install-terminfo,$(MAKECMDGOALS))): DESTDIR=$(DESTDIR) is this machine's root; name the directory it stages the files under)
endif
endif

.PHONY: install-build
install-build: ## Build what make install stages, for PREFIX (default /usr/local), with FEATURES='...' and CARGO_BUILD_FLAGS='...' when named
	VTIUM_FONT_HELPER=$(PREFIX)/bin/vtium-config $(if $(IMAGES),VTIUM_IMAGE=$(PREFIX)/bin/vtium-image) VTIUM_REVISION=$(or $(shell git describe --always --dirty 2>/dev/null),unknown) cargo rustc $(CARGO_BUILD_FLAGS) --profile minspec -p vtium --bin vtium --target-dir $(INSTALL_TARGET_DIR) $(if $(strip $(FEATURES)),--features '$(strip $(FEATURES))') -- $(RELR_LINK)
	$(call check-relr,$(INSTALL_TARGET_DIR)/minspec/vtium)
	grep -qaF '$(PREFIX)/bin/vtium-config' $(INSTALL_TARGET_DIR)/minspec/vtium || { echo "$(INSTALL_TARGET_DIR)/minspec/vtium does not start its helper at $(PREFIX)/bin/vtium-config" >&2; exit 1; }
# At PREFIX=/usr this path is also the one vtium is built with when
# VTIUM_IMAGE is unset, so there the check passes whether or not VTIUM_IMAGE
# reached the build, and the console is right either way. It still catches
# a console left from a build for another prefix, which lacks the path.
ifneq ($(IMAGES),)
	grep -qaF '$(PREFIX)/bin/vtium-image' $(INSTALL_TARGET_DIR)/minspec/vtium || { echo "$(INSTALL_TARGET_DIR)/minspec/vtium does not start its image helper at $(PREFIX)/bin/vtium-image" >&2; exit 1; }
endif
	cargo build $(CARGO_BUILD_FLAGS) --release -p vtium-config $(if $(IMAGES),-p vtium-image-helper) --target-dir $(INSTALL_TARGET_DIR)

.PHONY: install
install: install-build ## Stage what the vtium package ships under DESTDIR=DIR, for PREFIX (default /usr/local) and PKGNAME (default vtium): make install DESTDIR=DIR PREFIX=/usr
	install -Dm755 $(INSTALL_TARGET_DIR)/minspec/vtium $(DESTDIR)$(PREFIX)/bin/vtium
	install -Dm755 $(INSTALL_TARGET_DIR)/release/vtium-config $(DESTDIR)$(PREFIX)/bin/vtium-config
ifneq ($(IMAGES),)
	install -Dm755 $(INSTALL_TARGET_DIR)/release/vtium-image $(DESTDIR)$(PREFIX)/bin/vtium-image
	install -Dm644 $(VTIUM_INSTALL)/vtium-image.conf $(DESTDIR)$(PREFIX)/lib/sysusers.d/vtium-image.conf
endif
	tools/vt/units.sh --name vtium --exec $(PREFIX)/bin/vtium --by 'make install' --conflicts 'kmsconvt@%i.service' $(DESTDIR)$(UNITDIR)
	install -Dm644 $(VTIUM_INSTALL)/vtium.conf.example $(DESTDIR)/etc/vtium/vtium.conf.example
	install -Dm644 -t $(DESTDIR)$(PREFIX)/share/doc/vtium/examples $(VTIUM_INSTALL)/examples/71-vtium-local.rules $(VTIUM_INSTALL)/examples/vtium.hwdb
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/$(PKGNAME)/LICENSE

.PHONY: install-terminfo
install-terminfo: ## Stage vtium's terminfo entry, compiled as xterm-vtium, under DESTDIR=DIR for PREFIX and PKGNAME (default vtium-terminfo) (needs tic)
	@[ -f '$(TERMINFO_SRC)' ] || { echo "install-terminfo: no $(TERMINFO_SRC), the entry it compiles" >&2; exit 1; }
	install -dm755 $(DESTDIR)$(PREFIX)/share/terminfo
	tic -x -e xterm-vtium -o $(DESTDIR)$(PREFIX)/share/terminfo $(TERMINFO_SRC)
	@[ -f '$(DESTDIR)$(PREFIX)/share/terminfo/x/xterm-vtium' ] && [ -z "$$(find '$(DESTDIR)$(PREFIX)/share/terminfo' -type f ! -path '*/x/xterm-vtium')" ] || { echo "install-terminfo: tic did not compile xterm-vtium alone" >&2; exit 1; }
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/$(if $(filter command line,$(origin PKGNAME)),$(PKGNAME),vtium-terminfo)/LICENSE

# The ARM single-board computers minspec is judged against
# (docs/architecture.md): each target links against a pinned Debian sysroot
# and runs its tests under qemu-user, through tools/arm's wrappers
# (tools/arm/README.md). The sysroots live in a cache every checkout shares.
# The wrappers are named here, for these targets alone, so a native build on
# an ARM machine uses its own toolchain.
ARM_TARGETS      ?= aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf
ARM_TEST_TARGETS ?= aarch64-unknown-linux-gnu
arm arm-test: export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = $(ARM_WRAP)/aarch64-unknown-linux-gnu-cc
arm arm-test: export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER = $(ARM_WRAP)/aarch64-unknown-linux-gnu-run
arm arm-test: export PKG_CONFIG_aarch64_unknown_linux_gnu = $(ARM_WRAP)/aarch64-unknown-linux-gnu-pkg-config
arm arm-test: export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER = $(ARM_WRAP)/armv7-unknown-linux-gnueabihf-cc
arm arm-test: export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_RUNNER = $(ARM_WRAP)/armv7-unknown-linux-gnueabihf-run
arm arm-test: export PKG_CONFIG_armv7_unknown_linux_gnueabihf = $(ARM_WRAP)/armv7-unknown-linux-gnueabihf-pkg-config
# Each target's sysroot directory, which moves when its pins change. The
# pkg-config crate reruns a build script when this changes, so libudev-sys
# never keeps a search path into a sysroot the pins have left.
arm arm-test: export PKG_CONFIG_SYSROOT_DIR_aarch64_unknown_linux_gnu := $(shell tools/arm/sysroot dir arm64)
arm arm-test: export PKG_CONFIG_SYSROOT_DIR_armv7_unknown_linux_gnueabihf := $(shell tools/arm/sysroot dir armhf)

.PHONY: arm-sysroot
arm-sysroot: ## Fetch and verify the Debian sysroots the ARM targets link against (VTIUM_ARM_SYSROOTS=DIR)
	tools/arm/sysroot fetch arm64 armhf

# The oldest Rust vtium builds on: the rust-version Cargo.toml declares. The
# build has a target directory of its own, so the gates' other builds keep
# theirs.
MSRV := $(shell sed -n 's/^rust-version = "\(.*\)"$$/\1/p' Cargo.toml)

.PHONY: msrv
msrv: ## Build the workspace, its tests and the minspec profile with the oldest Rust vtium builds on, Cargo.toml's rust-version, in target/msrv (rustup installs that Rust when it is missing)
	@[ -n "$(MSRV)" ] || { echo "msrv: Cargo.toml declares no rust-version" >&2; exit 1; }
	@rustup run $(MSRV) rustc --version >/dev/null 2>&1 || rustup toolchain install $(MSRV) --profile minimal --no-self-update
	cargo +$(MSRV) build --workspace --all-features --all-targets --locked --target-dir target/msrv
	cargo +$(MSRV) build --profile minspec --locked --target-dir target/msrv

.PHONY: arm
arm: arm-sysroot ## Build the minspec profile for each ARM target, target/TRIPLE/minspec (ARM_TARGETS='...'; needs clang)
	VTIUM_REVISION=$(or $(shell git describe --always --dirty 2>/dev/null),unknown) cargo build --profile minspec $(addprefix --target ,$(ARM_TARGETS))
	for t in $(ARM_TARGETS); do \
		VTIUM_REVISION=$(or $(shell git describe --always --dirty 2>/dev/null),unknown) cargo rustc --profile minspec -p vtium --bin vtium --target $$t -- $(RELR_LINK) || exit 1; \
		$(call check-relr,$(or $(CARGO_TARGET_DIR),target)/$$t/minspec/vtium) || exit 1; \
		$(call check-glibc,$(or $(CARGO_TARGET_DIR),target)/$$t/minspec/vtium) || exit 1; \
	done

.PHONY: arm-test
arm-test: arm-sysroot ## Run the workspace's tests for each ARM target under qemu-user (ARM_TEST_TARGETS='...'; needs clang, qemu-user)
	cargo test --workspace --all-features --no-fail-fast $(addprefix --target ,$(ARM_TEST_TARGETS))

# Tier 1 and the capability probes, against the goldens and probes in
# tools/conformance/, copied from the C reference (provenance.tsv).
.PHONY: conformance
conformance: ## Judge vtium against tier 1's goldens
	cargo build --release -p vtium-conformance --features images --bins
	$(or $(CARGO_TARGET_DIR),target)/release/conformance run

.PHONY: conformance-probes
conformance-probes: ## Run the capability probes through vtium-replay
	cargo build --release -p vtium-conformance --features images --bins
	$(or $(CARGO_TARGET_DIR),target)/release/conformance probes

# Records real programs through a pty with this machine's bash, less, vim
# and tmux; not a gate.
.PHONY: conformance-record
conformance-record: ## Record the corpus's programs into OUT=DIR, each <name>.bin and <name>.cuts (NAMES='prog-shell ...', default all)
	$(if $(OUT),,$(error conformance-record needs OUT=DIR))
	cargo build --release -p vtium-conformance --features record --bin conformance-record
	$(or $(CARGO_TARGET_DIR),target)/release/conformance-record $(OUT) $(NAMES)

# The provenance checks, conformance-provenance, tier2-provenance and
# tier3-provenance, read libtsm and kmscon checkouts with git, read-only, to
# review a change to the copies; none is a gate.
LIBTSM ?= ../libtsm
KMSCON ?= ../kmscon

.PHONY: conformance-provenance
conformance-provenance: ## Check each row of tools/conformance/provenance.tsv against its source with git show (LIBTSM=DIR KMSCON=DIR)
	cargo build --release -p vtium-conformance --bins
	$(or $(CARGO_TARGET_DIR),target)/release/conformance provenance --libtsm $(LIBTSM) --kmscon $(KMSCON)

.PHONY: conformance-issues
conformance-issues: ## Check that every issue the conformance lists say owes a case or a capability is open (needs gh)
	cargo build --release -p vtium-conformance --bins
	$(or $(CARGO_TARGET_DIR),target)/release/conformance issues

.PHONY: test-tools
# shellcheck gates at warning: its info notes differ between versions, and
# the scripts' `A && B || die` is deliberate.
test-tools: ## Test the VT review tools, the tier 3 harness and the fuzz runner without a VT, sudo, logind, a VM or nightly (shellcheck when installed), the set writer without its deflate, that the console leaves the deflate out, and that make gates runs AGENTS.md's gate line
	python3 -I tools/vt/tests/test_vt.py
	python3 -I tools/vt/tests/test_accept.py
	python3 -I -B tools/tier3/test_t3img.py
	python3 -I -B tools/tier3/test_t3.py
	python3 -I -B tools/fuzz/test_run.py
	cargo test -q -p vtium-font --test set
	@if cargo tree -p vtium --target $(X86_TARGET) -e no-dev,features -i vtium-font | grep -q 'feature "deflate"'; then echo "the console enables vtium-font's deflate feature, which only vtium-config takes (ADR-1200 D19)" >&2; exit 1; else echo "the console links no deflate"; fi
	@$(check-gate-line)
	@if command -v shellcheck >/dev/null; then shellcheck -S warning -x tools/vt/*.sh tools/vt/checks/*.sh tools/vt/sections/*.sh tools/tier3/fetch-image.sh tools/tier3/fetch-debian.sh tools/tier3/guest/setup tools/tier3/guest/t3 tools/tier3/guest/t3-agent tools/tier3/guest/tier3-install tools/tier3/guest-sysvinit/setup tools/tier3/guest-sysvinit/tier3-install tools/tier3/guest-sysvinit/t3-console tools/arm/sysroot tools/arm/wrap && echo "shellcheck: clean at warning"; else echo "shellcheck: not installed, skipped"; fi

.PHONY: fuzz
# FUZZ_TARGETS and FUZZ_TIME reach tools/fuzz/run through the environment;
# its docstring describes the run, the corpus and the rows it appends.
fuzz: export FUZZ_TARGETS := $(value FUZZ_TARGETS)
fuzz: export FUZZ_TIME := $(value FUZZ_TIME)
fuzz: ## Run each fuzz target for 60 s, or FUZZ_TIME=N, and append a row per target to crates/CRATE/fuzz/runs.tsv (FUZZ_TARGETS='crate:target ...'; needs nightly and cargo-fuzz)
	tools/fuzz/run

# The review targets block until the run ends, then print its answers. Their
# variables reach tools/vt/vt.sh through the environment, unexpanded, never as
# recipe text: VT=N picks the VT, DEMO_ARGS='...' passes vtium options (split
# like shell words, quotes honoured, nothing expanded), DRY_RUN=1 prints the
# commands without running them, and VT_DEBUG=1 runs vtium with --debug and
# copies the kernel's DRM state beside the log.
$(VT_GOALS): export VTIUM_VT_BIN = $(value VTIUM_BIN)
$(VT_GOALS): export VTIUM_VT_HELPER = $(value VTIUM_CONFIG_BIN)
$(VT_GOALS): export VTIUM_VT_NR = $(value VT)
$(VT_GOALS): export VTIUM_VT_ARGS = $(value DEMO_ARGS)
$(VT_GOALS): export VTIUM_VT_DRY_RUN = $(value DRY_RUN)
$(VT_GOALS): export VTIUM_VT_DEBUG = $(value VT_DEBUG)
$(VT_GOALS): export VTIUM_VT_FEATURES = $(value FEATURES)
# With pictures, the image helper built with the console, which run-vt.sh
# gives vtium as it gives it the glyph helper.
$(VT_GOALS): export VTIUM_VT_IMAGE = $(if $(IMAGES),$(value VTIUM_IMAGE_BIN))
# GUIDE and GUIDE_STEPS are make vt-check's; one exported in the shell does
# not reach the others.
$(VT_GOALS): export VTIUM_VT_GUIDE =
$(VT_GOALS): export VTIUM_VT_STEPS =

.PHONY: vt-accept
vt-accept: export VTIUM_VT_SESSION = accept
vt-accept: $(vt_build) $(vt_helper_build) $(vt_image_build) ## Run the acceptance pass on the development VT or a free VT, with pictures unless FEATURES is named, every page answered and the pass recorded, then print the pass (VT=N, FEATURES='...', DEMO_ARGS='...', DRY_RUN=1, VT_DEBUG=1)
	tools/vt/vt.sh

.PHONY: vt-login
vt-login: export VTIUM_VT_SESSION = login
vt-login: $(vt_build) $(vt_helper_build) ## Run vtium with a login prompt on the development VT or a free VT, until the first logout (VT=N, DEMO_ARGS='...', VT_DEBUG=1)
	tools/vt/vt.sh

.PHONY: vt-shell
vt-shell: export VTIUM_VT_SESSION = shell
vt-shell: $(vt_build) $(vt_helper_build) ## Run your login shell, as you, in vtium on the development VT or a free VT; exit it to end the run (VT=N, DEMO_ARGS='...', VT_DEBUG=1)
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
vt-check: $(vt_build) $(vt_helper_build) ## Run a guide's checks on the development VT or a free VT; the operator answers (GUIDE=NAME, GUIDE_STEPS='2 3', VT_DEBUG=1)
	tools/vt/vt.sh

# A development build of vtium as the console of one VT, across reboots,
# under names of its own, vtium-dev@ and /usr/local/bin/vtium-dev, beside the
# packaged vtium (docs/vt-review.md, "Installing vtium on a VT"): VT=N names
# it, and DRY_RUN=1 prints the units and every command without running any.
# Their variables reach tools/vt/vt-install.sh through the environment, never
# as recipe text. Run as root, the build would leave root's files in target/,
# and the script asks sudo for each step that needs root, so root is refused
# before anything is built. The tests, whose stand-ins write only under their
# own root (VTIUM_VT_STANDINS), may run as root, but make vt-install then
# needs the binary and the helpers named with VTIUM_BIN, VTIUM_CONFIG_BIN
# and, with images in FEATURES, VTIUM_IMAGE_BIN, so that no build runs as
# root.
ifneq ($(filter $(VT_INSTALL_GOALS),$(MAKECMDGOALS)),)
ifeq ($(shell id -u),0)
ifeq ($(VTIUM_VT_STANDINS),)
$(error $(firstword $(filter $(VT_INSTALL_GOALS),$(MAKECMDGOALS))): run it as yourself, not as root or under sudo; it asks sudo for each step that needs root)
endif
ifneq ($(and $(filter vt-install,$(MAKECMDGOALS)),$(strip $(vt_build) $(vt_dev_build))),)
$(error vt-install: as root under the tests' stand-ins, name the binary and the helper with VTIUM_BIN=PATH VTIUM_CONFIG_BIN=PATH, and with images in FEATURES the image helper with VTIUM_IMAGE_BIN=PATH, so that no build runs as root and leaves root's files in target/)
endif
endif
endif
$(VT_INSTALL_GOALS): export VTIUM_VT_BIN = $(value VTIUM_BIN)
$(VT_INSTALL_GOALS): export VTIUM_VT_NR = $(value VT)
$(VT_INSTALL_GOALS): export VTIUM_VT_CONFIG = $(value VTIUM_CONFIG_BIN)
$(VT_INSTALL_GOALS): export VTIUM_VT_IMAGE = $(value VTIUM_IMAGE_BIN)
$(VT_INSTALL_GOALS): export VTIUM_VT_DRY_RUN = $(value DRY_RUN)

# The development console: the minspec build, with FEATURES, made to start
# its helper at VTIUM_DEV_HELPER and, with images in FEATURES, its image
# helper at VTIUM_DEV_IMAGE, which vt-install installs. The console carries
# VT_IMAGES_MARKER exactly when it is built with images, as vt-install reads
# it.
.PHONY: vt-dev-console
vt-dev-console: ## Build the console make vt-install installs, target/vt-dev/x86_64-unknown-linux-gnu/minspec/vtium, with FEATURES='...' when named (needs clang)
	tools/arm/sysroot fetch amd64
	VTIUM_FONT_HELPER=$(VTIUM_DEV_HELPER) $(if $(IMAGES),VTIUM_IMAGE=$(VTIUM_DEV_IMAGE)) VTIUM_REVISION=$(or $(shell git describe --always --dirty 2>/dev/null),unknown) cargo rustc --profile minspec -p vtium --bin vtium --target $(X86_TARGET) --target-dir $(VTIUM_DEV_DIR) $(if $(strip $(FEATURES)),--features '$(strip $(FEATURES))') -- $(RELR_LINK)
	$(call check-relr,$(VTIUM_DEV))
	$(call check-glibc,$(VTIUM_DEV))
ifneq ($(IMAGES),)
	grep -qaF '$(VTIUM_DEV_IMAGE)' $(VTIUM_DEV) || { echo "$(VTIUM_DEV) does not start its image helper at $(VTIUM_DEV_IMAGE)" >&2; exit 1; }
	grep -qaF '$(VT_IMAGES_MARKER)' $(VTIUM_DEV) || { echo "$(VTIUM_DEV) is built with images but lacks the line '$(VT_IMAGES_MARKER)' that vt-install tells such a build by (tools/vt/vt-install.sh)" >&2; exit 1; }
else
	! grep -qaF '$(VT_IMAGES_MARKER)' $(VTIUM_DEV) || { echo "$(VTIUM_DEV) is built without images but carries the line '$(VT_IMAGES_MARKER)' that vt-install tells a build with images by (tools/vt/vt-install.sh)" >&2; exit 1; }
endif

.PHONY: vt-install
vt-install: $(vt_build) $(vt_dev_build) ## Install a development build as vtium-dev on VT N in place of its getty, started at every boot (VT=N, DRY_RUN=1, FEATURES='...')
	@case "$$VTIUM_VT_DRY_RUN" in '') tools/vt/vt-install.sh install "$$VTIUM_VT_NR" ;; 1) tools/vt/vt-install.sh install --dry-run "$$VTIUM_VT_NR" ;; *) echo "DRY_RUN=$$VTIUM_VT_DRY_RUN: 1 or nothing" >&2; exit 1 ;; esac

.PHONY: vt-uninstall
vt-uninstall: ## Undo make vt-install on VT N: vtium-dev off, its getty back, the files it wrote removed (VT=N, DRY_RUN=1)
	@case "$$VTIUM_VT_DRY_RUN" in '') tools/vt/vt-install.sh uninstall "$$VTIUM_VT_NR" ;; 1) tools/vt/vt-install.sh uninstall --dry-run "$$VTIUM_VT_NR" ;; *) echo "DRY_RUN=$$VTIUM_VT_DRY_RUN: 1 or nothing" >&2; exit 1 ;; esac

# Tier 2 against vtium's goldens and the C reference's, copied into
# tools/tier2/reference (provenance.tsv).
.PHONY: tier2
tier2: ## Draw tier 2's screens and judge them against their goldens
	cargo build --release -p vtium-tier2 --bins
	$(or $(CARGO_TARGET_DIR),target)/release/tier2 run

# The demonstration set (vtium/vtium#142): the test card drawn for each
# display profile into docs/specimens/, and judged against what is committed.
# The outline profiles draw the glyphs vtium-config's engine generates from the pinned fonts
# specimens-fonts fetches into $XDG_CACHE_HOME/vtium/specimens/fonts.
.PHONY: specimens specimens-check specimens-fonts
specimens: ## Draw the test card for each display profile into docs/specimens (REVIEW=DIR copies each collage there)
	cargo build --release -p vtium-specimens -p vtium-config --bins
	$(or $(CARGO_TARGET_DIR),target)/release/specimens draw --helper $(or $(CARGO_TARGET_DIR),target)/release/vtium-config $(if $(REVIEW),--review $(REVIEW))

specimens-check: ## Draw the specimens again: an accepted profile that moved fails, a provisional one reports drift
	cargo build --release -p vtium-specimens -p vtium-config --bins
	$(or $(CARGO_TARGET_DIR),target)/release/specimens check --helper $(or $(CARGO_TARGET_DIR),target)/release/vtium-config

specimens-fonts: ## Fetch the outline profiles' pinned fonts into the cache, checking each SHA-256
	cargo build --release -p vtium-specimens --bins
	$(or $(CARGO_TARGET_DIR),target)/release/specimens fetch-fonts

.PHONY: tier2-provenance
tier2-provenance: ## Check tools/tier2's copies against their sources with git show (KMSCON=DIR)
	cargo build --release -p vtium-tier2 --bins
	$(or $(CARGO_TARGET_DIR),target)/release/tier2 provenance --kmscon $(KMSCON)

# Tier 3: vtium's install tree in a QEMU/KVM guest with a real kernel VT and
# KMS device, its screens captured and compared with the C reference's
# goldens copied into tools/tier3/c-goldens, and vtium's own
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
# target/tier3/vm/images links to, and where tier3-image fetches the pristine
# image and builds the base image.
T3_IMAGES   ?= $(or $(XDG_CACHE_HOME),$(HOME)/.cache)/vtium/tier3/images

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
# FEATURES, when make's command line names it, even empty, names the
# features the tree's console is built with, or was built with for
# tier3-run; the budget judges a tree built without any. tier3-run on the
# last run's tree takes them from that run's record and refuses a FEATURES
# that differs.
T3_FEATURES = $(if $(filter command line,$(origin FEATURES)),--features '$(strip $(FEATURES))')
T3_RUN = T3_VTIUM_REV=$(T3_REV) T3_IMAGE=$(T3_IMAGE) T3_IMAGE_SHA256=$(T3_IMAGE_SHA256) \
	T3_ARCHIVE=$(T3_ARCHIVE) $(T3_PY) $(T3_DIR)/t3.py run --jobs $(JOBS) \
	--deadline $(T3_DEADLINE) $(T3_FEATURES)

.PHONY: tier3-image
tier3-image: ## Fetch, verify and provision tier 3's guest image, unless the cache holds one built from the same inputs (T3_IMAGES=DIR)
	$(T3_PY) $(T3_DIR)/t3.py provision --image $(T3_IMAGE) --archive $(T3_ARCHIVE) \
		--url $(T3_IMAGE_URL) --sha256 $(T3_IMAGE_SHA256) --key $(T3_IMAGE_KEY) \
		--images $(T3_IMAGES)

# tier3 and tier3-minimal name the tree's target for t3.py, which builds it
# under the run lock, so a second run cannot rebuild the tree under a running
# one. The recipes hold no $(MAKE), so `make -n` runs nothing.
.PHONY: tier3
tier3: tier3-image ## Tier 3: build the install tree, then run every variant, or VARIANTS="..." (JOBS=N, FEATURES='...')
	$(T3_RUN) --tree-target install-tree $(VARIANTS)

.PHONY: tier3-minimal
tier3-minimal: tier3-image ## Tier 3 on the minspec machine, 1 vCPU and 512 MB: unifont, a PSF font, and the 320x240 and 640x480 displays
	$(T3_RUN) --tree-target install-tree $(or $(VARIANTS),2d-potato-min 2d-potato-min-psf 2d-320x240-psf8 2d-640x480-psf16 2d-640x480-frit 2d-640x480-frit-held)

# vtium's pictures on tier 3: the tree's console built with images, and with
# any other features FEATURES names, then the images variants, which draw
# sixel and kitty pictures and pictures the image helper decodes, and check
# the helper's account before login.
.PHONY: tier3-images
tier3-images: tier3-image ## Tier 3's pictures: the install tree built with images, then the images variants on the minspec machine at 640x480
	$(T3_RUN) --features '$(sort images $(subst $(comma), ,$(FEATURES)))' --tree-target install-tree $(or $(VARIANTS),images-640x480 images-640x480-nobody)

# Each row of tools/tier3/provenance.tsv against its source in a kmscon
# checkout, read with git; a review step when the copies change, never a gate.
.PHONY: tier3-provenance
tier3-provenance: ## Check tools/tier3's copies against their sources with git (KMSCON=DIR)
	$(T3_PY) $(T3_DIR)/t3.py provenance --kmscon $(KMSCON)

.PHONY: tier3-run
tier3-run: ## Tier 3 on a tree built already: the last run's, or TREE=DIR (VARIANTS=, JOBS= as for tier3; FEATURES= the TREE was built with)
	$(T3_RUN) $(if $(TREE),--tree $(TREE)) $(VARIANTS)

# Tier 3 on the guest without systemd (tools/tier3/README.md, docs/systemd.md):
# Debian 13's generic cloud image, pinned by its build and SHA-256, with its
# packages from snapshot.debian.org at a pinned time and sysvinit as PID 1.
# Debian signs no cloud image; the SHA-256 is of the image whose SHA-512
# matched the build's SHA512SUMS. Its base image is a second one, beside the
# Arch guest's, so the run is beside tier3-minimal's.
T3SV_IMAGE_VER    := 20261015-2632
T3SV_IMAGE        := debian-13-generic-amd64-$(T3SV_IMAGE_VER).qcow2
T3SV_IMAGE_URL    := https://cloud.debian.org/images/cloud/trixie/$(T3SV_IMAGE_VER)
T3SV_IMAGE_SHA256 := 1010eb3215be632548cfb6793b53bd8243064d8820b5106f2becfcd179099b2f
T3SV_SNAPSHOT     := 20261016T000000Z

.PHONY: tier3-image-nosystemd
tier3-image-nosystemd: ## Fetch, verify and provision tier 3's guest without systemd, unless the cache holds it (T3_IMAGES=DIR)
	$(T3_PY) $(T3_DIR)/t3.py provision --guest sysvinit --image $(T3SV_IMAGE) \
		--archive $(T3SV_SNAPSHOT) --url $(T3SV_IMAGE_URL) --sha256 $(T3SV_IMAGE_SHA256) \
		--images $(T3_IMAGES)

.PHONY: tier3-nosystemd
tier3-nosystemd: tier3-image-nosystemd ## Tier 3 on the guest without systemd: vtium under sysvinit, without udev, and its power keys
	T3_VTIUM_REV=$(T3_REV) T3_IMAGE=$(T3SV_IMAGE) T3_IMAGE_SHA256=$(T3SV_IMAGE_SHA256) \
		T3_ARCHIVE=$(T3SV_SNAPSHOT) $(T3_PY) $(T3_DIR)/t3.py run --jobs $(JOBS) \
		--deadline $(T3_DEADLINE) $(T3_FEATURES) --tree-target install-tree $(or $(VARIANTS),sysvinit-640x480 sysvinit-no-udev sysvinit-reboot sysvinit-restart sysvinit-power-off)

# AGENTS.md's gate line, one gate per line, in its order; make test-tools
# fails when the two differ (check-gate-line).
define GATE_LINE
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo test --workspace --all-features
cargo test --workspace
cargo build --profile minspec
make msrv
docs/scripts/adr lint --check
docs/scripts/adr cite --check
docs/scripts/issue-refs
docs/scripts/coverage
make test-tools
make conformance
make conformance-probes
make conformance-issues
make tier2
make arm
endef
export GATE_LINE

# check-gate-line reads the fenced block after "Every change runs the full
# gate line" in AGENTS.md, one gate per line without its `&&` and `\`, and
# fails unless it is GATE_LINE, which reaches awk through the environment.
define check-gate-line
awk 'f && /^```/ { f = 0; done = 1 } f { sub(/^ *(&& )?/, ""); sub(/ \\$$/, ""); got = got $$0 "\n" } /^## The gates$$/ { g = 1 } g && !done && /^```sh$$/ { f = 1 } END { want = ENVIRON["GATE_LINE"] "\n"; if (got == "" || got != want) { printf "make gates runs another gate line than AGENTS.md lists.\nAGENTS.md:\n%sGATE_LINE:\n%s", got, want > "/dev/stderr"; exit 1 } print "make gates runs AGENTS.md'"'"'s gate line" }' AGENTS.md
endef

.PHONY: gates
gates: ## Run every gate of the gate line, each to the end, report each exit code and fail if any failed
	@[ -n "$$(printf '%s' "$$GATE_LINE" | tr -d '[:space:]')" ] || { echo "gates: the gate list is empty" >&2; exit 1; }; \
	summary=$$(mktemp); trap 'rm -f "$$summary"' EXIT; \
	printf '%s\n' "$$GATE_LINE" | while IFS= read -r gate; do \
		echo "==> $$gate"; \
		sh -c "$$gate" </dev/null; code=$$?; \
		echo "$$code  $$gate" >> "$$summary"; \
	done; \
	echo; echo "exit  gate"; cat "$$summary"; \
	if grep -qv '^0  ' "$$summary"; then echo "gates: FAILED" >&2; exit 1; fi; \
	echo "gates: all passed"
