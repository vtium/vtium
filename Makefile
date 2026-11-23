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
CLIPBOARD_RELEASE := $(or $(CARGO_TARGET_DIR),target)/$(X86_TARGET)/release/vtium-clipboard
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
# CONF names the vtium.conf that vtium, as root, reads in a review run, and
# that file's login line is a command vtium runs. Other tools export a
# variable of that name too, so the targets that read CONF take it only from
# make's command line.
ifneq ($(filter vt-login vt-shell vt-check,$(MAKECMDGOALS)),)
ifneq ($(filter environment%,$(origin CONF)),)
$(error CONF is set in the environment; the review run gives the file to vtium, which runs as root, so name it on make's command line: make $(firstword $(MAKECMDGOALS)) CONF=PATH)
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

# The minspec console binary also links with identical code folding: the
# linker keeps one copy of each set of functions whose machine code and
# relocations are the same, and points every caller at that copy. Generic
# code instantiated for types of the same layout folds this way, and the
# binary shrinks by about 42 KB, which keeps it under the 4,390,912-byte
# budget that tier 3 holds the minspec binary to. The mode is all, not safe:
# safe folds only functions whose address no code takes, and rustc emits no
# address-significance table to tell the linker which those are, so safe
# folds nothing. Folding gives two functions one address, so code that
# compares function addresses could see two functions as one; no code in the
# minspec binary or the crates it links compares them. The flag needs lld,
# which the tools/arm wrappers link through, so it goes to the minspec
# builds that link through those wrappers: make minspec, make install-tree,
# make vt-dev-console and make arm. make install-build links with the host's
# linker, which may be GNU ld, and GNU ld has no --icf, so it links without
# folding.
#
# References:
# - ADR-1200 D19: the minspec binary's size budget.
ICF_LINK := -C link-arg=-Wl,--icf=all

# The minspec builds also shorten the source paths that rustc writes into
# the binary. A panic message names the file and line it comes from, so the
# binary holds the path of each dependency's source file that can panic,
# such as ~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/calloop-0.14.4/src/sys.rs.
# The first --remap-path-prefix writes the cargo home, such as a git
# checkout of a dependency, under /cargo. The second writes the crates.io
# sources as cargo's own trim-paths option does, from the crate's directory
# on, as calloop-0.14.4/src/sys.rs. rustc applies the last prefix that
# matches a path, so the crates.io prefix, the more specific one, comes
# last and wins over the cargo home's. The binary shrinks
# by about 2.6 KB, which counts against the 4,390,912-byte budget that tier
# 3 holds the minspec binary to, and its size no longer depends on where
# the cargo home is. vtium's own sources already appear as paths relative
# to the workspace, such as crates/vtium/src/main.rs. The flags go to every
# crate of the build through cargo's build.rustflags, which a RUSTFLAGS in
# the environment replaces, so check-paths fails a build that lost them.
# The directory name of the crates.io sources is the hash cargo gives
# crates.io, so check-paths also fails when a new cargo names it otherwise.
#
# References:
# - ADR-1200 D19: the minspec binary's size budget.
CARGO_HOME_DIR := $(or $(CARGO_HOME),$(HOME)/.cargo)
CRATES_IO_SRC := $(CARGO_HOME_DIR)/registry/src/index.crates.io-1949cf8c6b5b557f
REMAP_PATHS := --config 'build.rustflags = ["--remap-path-prefix=$(CARGO_HOME_DIR)=/cargo", "--remap-path-prefix=$(CRATES_IO_SRC)/="]'

# check-paths FILE fails when FILE holds the path of a crate's source in a
# cargo registry, so the shortened paths cannot drop silently.
define check-paths
! grep -qaF '/registry/src/' $(1) || { echo "$(1) holds the path of a crate's source in the cargo registry: the minspec build lost --remap-path-prefix (is RUSTFLAGS set?), or cargo names the crates.io sources' directory otherwise than $(CRATES_IO_SRC)" >&2; exit 1; }
endef

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
minspec install-tree vt-dev-console config-release image-release clipboard-release: export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = $(ARM_WRAP)/$(X86_TARGET)-cc
minspec install-tree vt-dev-console config-release image-release clipboard-release: export PKG_CONFIG_x86_64_unknown_linux_gnu = $(ARM_WRAP)/$(X86_TARGET)-pkg-config
# The sysroot's directory, which moves when its pins change, so the
# pkg-config crate reruns a build script whose search path it would leave,
# as the arm targets' do below.
minspec install-tree vt-dev-console config-release image-release clipboard-release: export PKG_CONFIG_SYSROOT_DIR_x86_64_unknown_linux_gnu := $(shell tools/arm/sysroot dir amd64)

# The vtium crate's features the console binary is built with, named on
# make's command line (make install-tree FEATURES='images logind'). None, the
# default, builds the minspec console, the only binary tier 3's budget judges
# (ADR-1200 D19). The assignment here outranks the environment, so a
# FEATURES exported in the shell builds nothing different.
FEATURES :=
# The acceptance pass shows every page by default (ADR-1501 D5): make
# vt-accept builds the console with images and bidi unless make's command
# line names FEATURES, so make vt-accept FEATURES= is a pass of the minspec
# build. A binary named with VTIUM_BIN is run as it is, so its features are
# the ones it was built with: images when it carries the line only a build
# with images carries (VT_IMAGES_MARKER), else none, and the run gives it
# the image helper only when it has pictures.
#
# RENDER_PATH chooses the path the pass judges: cpu, the default, runs
# vtium with --no-hwaccel, and gpu runs it with --hwaccel, so that one batch
# gets a pass on each path (AGENTS.md, V3). The variable is named on make's
# command line; one exported in the shell is not read. RENDER_PATH=gpu adds
# gpu to the default features and refuses FEATURES named without it. A
# binary named with VTIUM_BIN is run as it is, and vtium shows a notice
# when it lacks the GPU path that --hwaccel asks for.
comma := ,
ifneq ($(filter vt-accept,$(MAKECMDGOALS)),)
ifneq ($(origin RENDER_PATH),command line)
RENDER_PATH := cpu
endif
ifeq ($(filter cpu gpu,$(RENDER_PATH)),)
$(error vt-accept: RENDER_PATH is gpu or cpu, not '$(RENDER_PATH)')
endif
ifneq ($(word 2,$(RENDER_PATH)),)
$(error vt-accept: RENDER_PATH is gpu or cpu, not '$(RENDER_PATH)')
endif
ifneq ($(origin FEATURES),command line)
ifeq ($(filter $(VTIUM_MINSPEC),$(VTIUM_BIN)),)
FEATURES := $(if $(shell grep -qaF -- '$(VT_IMAGES_MARKER)' '$(VTIUM_BIN)' 2>/dev/null && echo images),images)
else
FEATURES := images bidi$(if $(filter gpu,$(RENDER_PATH)), gpu)
endif
else ifeq ($(RENDER_PATH),gpu)
ifeq ($(filter gpu,$(subst $(comma), ,$(FEATURES))),)
$(error vt-accept: RENDER_PATH=gpu needs the GPU path in the console: name gpu in FEATURES, or leave FEATURES out)
endif
endif
endif

.PHONY: minspec
minspec: ## Build the minspec binary for x86_64 against the glibc 2.36 sysroot, target/x86_64-unknown-linux-gnu/minspec/vtium, with FEATURES='...', HWACCEL=on|off and DISPLAY_BUFFERS=1|2|3 when named (needs clang)
	tools/arm/sysroot fetch amd64
	$(build_settings)VTIUM_REVISION=$(or $(shell git describe --always --dirty 2>/dev/null),unknown) cargo rustc --profile minspec $(REMAP_PATHS) -p vtium --bin vtium --target $(X86_TARGET) $(if $(strip $(FEATURES)),--features '$(strip $(FEATURES))') -- $(RELR_LINK) $(ICF_LINK)
	$(call check-relr,$(VTIUM_MINSPEC))
	$(call check-paths,$(VTIUM_MINSPEC))
	$(call check-glibc,$(VTIUM_MINSPEC))

# vtium as a system installs it, staged under DESTDIR: the console binary,
# built with the minspec profile and FEATURES, vtium-config, the clipboard
# program vtium-clipboard and its short name vticlip, a link to it
# (ADR-1007 D1), with images in
# FEATURES the image helper vtium-image and its sysusers file, which creates
# its account for the login prompt (ADR-1101 D1, D2), the VT unit
# vtium@.service and its getty fallback vtium-getty@.service, generated by
# tools/vt/units.sh as make install generates them (ADR-1500 D3), the
# compiled terminfo entry, the example configuration
# /etc/vtium/vtium.conf.example beside the file vtium reads,
# /etc/vtium/vtium.conf (ADR-1500 D3, D4), the font sets FRITS names in
# their category folders under /etc/vtium/fonts and the layers FRIT_LAYERS
# names in /etc/vtium/fonts/layers (ADR-1204 D9), the example udev rules
# and hwdb fragment, which are documentation and never active (ADR-1401
# D4), with images in
# FEATURES the picture gallery tool as vtium-gallery (ADR-1500 D8), and with
# libseat and logind in FEATURES seat mode's unit vtium-seat@.service and the
# PAM file /etc/pam.d/vtium (ADR-1400 D3), the man pages in /usr/share/man,
# and the package data install-package-data names: the color schemes, their
# license texts and the preset vtlooks. Tier 3 installs this tree in its
# guest.
# DESTDIR's default is install-tree's alone: make install refuses it below.
DESTDIR ?= target/install-tree
VTIUM_INSTALL := crates/vtium/install
# The image helper ships only with a console built with images: a minspec
# build installs none (ADR-1101 D1). FEATURES may separate its features
# with commas or spaces, as cargo's --features does (comma, above).
IMAGES := $(filter images,$(subst $(comma), ,$(FEATURES)))
# A console built with libseat and logind runs in direct mode through
# vtium@.service, or in seat mode through vtium-seat@.service, which opens a
# PAM session of vtium's own that libseat's logind backend takes the seat
# from (ADR-1400 D3). vtium console enable picks the mode for each VT
# (ADR-1500 D7). So with both features make install and make install-tree
# generate vtium-seat@.service beside vtium@.service, with units.sh --seat,
# and stage the PAM file /etc/pam.d/vtium, and only then. A console built
# with libseat alone is for seatd: it opens no PAM session, and its logins
# run as in the default build. PAM reads /etc/pam.d whatever PREFIX is.
PAM_SESSION := $(and $(filter libseat,$(subst $(comma), ,$(FEATURES))),$(filter logind,$(subst $(comma), ,$(FEATURES))))
# The font sets (frits) make install and make install-tree stage into
# /etc/vtium/fonts, each in its category's folder, and the layers they add
# in /etc/vtium/fonts/layers (ADR-1204 D9, ADR-1500 D8). Each set is
# NAME.vtfrit in a category folder of crates/vtium-config/frits, and each
# layer NAME.vtlayer in its layers folder. The default is the vtium
# package's: the two sets for text, terminus and cullet-mono, then
# jetbrains-mono and the small panels' two sets, and
# FRIT_LAYERS names the layers those sets add. Every console package, the
# minspec ones included, ships these lists. FRITS='' stages no set, and
# FRIT_LAYERS='' no layer. A set names its fonts.
FRIT_ROOT := crates/vtium-config/frits
FRIT_CATEGORIES := console classic code nerd scripts small
FRIT_DIRS := $(addprefix $(FRIT_ROOT)/,$(FRIT_CATEGORIES))
CONSOLE_FRITS := terminus cullet-mono jetbrains-mono unifont-icons unscii-icons
CONSOLE_FRIT_LAYERS := nerd-symbols nerd-symbols-small noto-emoji noto-emoji-small unifont
FRITS := $(CONSOLE_FRITS)
FRIT_LAYERS := $(CONSOLE_FRIT_LAYERS)
# The sets and layers the optional vtium-fonts package ships beside the
# fonts they name, which make install-fonts stages: the rest of the catalog.
# It leaves out every set and layer a console package can ship, so that
# pacman can install vtium-fonts beside any console package without two
# packages owning one file.
#
# References:
# - ADR-1204 D9: vtium-fonts ships the catalog beside the fonts it names.
FONTS_FRITS := $(filter-out $(CONSOLE_FRITS),$(sort $(basename $(notdir $(wildcard $(addsuffix /*.vtfrit,$(FRIT_DIRS)))))))
FONTS_LAYERS := $(filter-out $(CONSOLE_FRIT_LAYERS),$(sort $(basename $(notdir $(wildcard $(FRIT_ROOT)/layers/*.vtlayer)))))
frit_paths = $(foreach f,$(1),$(or $(firstword $(wildcard $(addsuffix /$(f).vtfrit,$(FRIT_DIRS)))),$(error no frit $(f) in $(FRIT_DIRS))))
layer_paths = $(foreach l,$(1),$(or $(wildcard $(FRIT_ROOT)/layers/$(l).vtlayer),$(error no layer $(l) in $(FRIT_ROOT)/layers)))
frit_files = $(call frit_paths,$(FRITS))
# The command that stages the sets $(1) in their category folders and the
# layers $(2) in the layers folder, under DESTDIR. frit_commands starts
# each install with &&, and install_frits drops the first &&, so with
# neither sets nor layers the command is empty and make runs nothing.
frit_commands = $(strip $(foreach c,$(FRIT_CATEGORIES),$(if $(filter $(FRIT_ROOT)/$(c)/%,$(1)),&& install -Dm644 -t $(DESTDIR)/etc/vtium/fonts/$(c) $(filter $(FRIT_ROOT)/$(c)/%,$(1)))) $(if $(strip $(2)),&& install -Dm644 -t $(DESTDIR)/etc/vtium/fonts/layers $(2)))
install_frits = $(call drop_first,$(call frit_commands,$(1),$(2)))
drop_first = $(wordlist 2,$(words $(1)),$(1))
# A frit vtium does not hold stops the install as make reads this file,
# before anything is built.
ifneq ($(filter install install-build install-tree,$(MAKECMDGOALS)),)
_frits := $(frit_files) $(call layer_paths,$(FRIT_LAYERS))
endif
ifneq ($(filter install-fonts,$(MAKECMDGOALS)),)
_fonts_frits := $(call frit_paths,$(FONTS_FRITS)) $(call layer_paths,$(FONTS_LAYERS))
endif
# The font files every build installs in /usr/share/vtium/fonts, minspec
# builds included: bitmap Terminus 4.49.1's eighteen BDF files, gzipped,
# GNU Unifont's all-planes .hex file, and Cullet Mono's two faces, with
# Cullet Mono's notices and the license texts they name in the package's
# license directory. make font-files stages the Terminus and Unifont files
# in FONT_FILES from a pinned archive, and make cullet-mono builds the
# faces in CULLET_MONO_OUT. make install and make install-tree check every
# file against its pin first, and stop with a line that names the target
# that stages a missing file. tools/fonts/README.md describes the files and
# their pins.
#
# References:
# - ADR-1204 D2: every build installs the Terminus, Unifont and Cullet Mono
#   files, and make install writes the package's prefix into the paths
#   the font sets name.
# - ADR-1204 D8: vtium installs these fonts itself.
FONT_FILES := $(or $(CARGO_TARGET_DIR),target)/font-files
# The script that stages and checks them; the tests name a stand-in.
FONT_FILES_TOOL := tools/fonts/files
FONT_FILE_NAMES := $(foreach h,12 14 16 18 20 22 24 28 32,ter-u$(h)n.bdf.gz ter-u$(h)b.bdf.gz) unifont_all-17.0.03.hex
CULLET_MONO_FACES := CulletMono-Regular.ttf CulletMono-Bold.ttf
CULLET_MONO_NOTICES := $(sort $(wildcard tools/cullet-mono/notices/*))
# install-font-files,FONTS,LICENSES stages the font files in FONTS and
# Cullet Mono's notices in LICENSES/cullet-mono.
define install-font-files
install -Dm644 -t $(1) $(addprefix $(FONT_FILES)/,$(FONT_FILE_NAMES)) $(addprefix $(CULLET_MONO_OUT)/,$(CULLET_MONO_FACES))
install -Dm644 -t $(2)/cullet-mono $(CULLET_MONO_NOTICES)
endef

# The picture gallery tool, which a console with images ships as
# vtium-gallery (ADR-1500 D8).
GALLERY := tools/gallery/gallery

.PHONY: config-release
config-release: ## Build vtium-config, the font tool and glyph helper, for x86_64 against the glibc 2.36 sysroot, target/x86_64-unknown-linux-gnu/release/vtium-config
	$(build_settings)cargo build --release -p vtium-config --target $(X86_TARGET)
	$(call check-glibc,$(CONFIG_RELEASE))

.PHONY: image-release
image-release: ## Build vtium-image, the image helper, for x86_64 against the glibc 2.36 sysroot, target/x86_64-unknown-linux-gnu/release/vtium-image
	$(build_settings)cargo build --release -p vtium-image-helper --target $(X86_TARGET)
	$(call check-glibc,$(IMAGE_RELEASE))

.PHONY: clipboard-release
clipboard-release: ## Build vtium-clipboard, the program that copies text to the console's clipboard, for x86_64 against the glibc 2.36 sysroot, target/x86_64-unknown-linux-gnu/release/vtium-clipboard
	cargo build --release -p vtium-clipboard --target $(X86_TARGET)
	$(call check-glibc,$(CLIPBOARD_RELEASE))

# The color schemes every build ships, each NAME.palette in Ghostty's theme
# format, the license text of each scheme's family, and the preset vtlooks.
# install-package-data,SHARE,LICENSES stages the schemes in SHARE/palettes,
# the presets in SHARE/looks and the license texts in LICENSES/palettes, where
# SHARE is the package's data folder, /usr/share/vtium for a system. The
# presets are taken by wildcard, and an empty folder of presets stages none.
#
# References:
# - ADR-1205 D10: the package's color schemes in /usr/share/vtium/palettes,
#   each scheme's license text installed with the package's other notices.
# - ADR-1304 D4, its note of 2026-11-19: the package's preset vtlooks in
#   /usr/share/vtium/looks.
PACKAGE_PALETTES := $(sort $(wildcard $(VTIUM_INSTALL)/palettes/*.palette))
PALETTE_LICENSES := $(sort $(wildcard $(VTIUM_INSTALL)/palettes/licenses/*))
PACKAGE_LOOKS := $(sort $(wildcard $(VTIUM_INSTALL)/looks/*.vtlook))
define install-package-data
install -Dm644 -t $(1)/palettes $(PACKAGE_PALETTES)
install -Dm644 -t $(2)/palettes $(PALETTE_LICENSES)
$(if $(PACKAGE_LOOKS),install -Dm644 -t $(1)/looks $(PACKAGE_LOOKS))
endef

# install-man,DIR installs the man pages every build ships under DIR, the
# man directory, with vticlip.1 a link to vtium-clipboard.1 as vticlip is to
# vtium-clipboard. The gallery's page goes with the gallery.
define install-man
install -Dm644 -t $(1)/man8 $(MAN_DIR)/vtium.8
install -Dm644 -t $(1)/man1 $(MAN_DIR)/vtium-config.1 $(MAN_DIR)/vtium-clipboard.1
ln -sfn vtium-clipboard.1 $(1)/man1/vticlip.1
install -Dm644 -t $(1)/man5 $(MAN_DIR)/vtium.conf.5
endef

.PHONY: install-tree
install-tree: font-files-check minspec config-release clipboard-release $(if $(IMAGES),image-release) ## Stage vtium's install tree under DESTDIR (default target/install-tree), its console built with FEATURES='...' and its frits FRITS='...' when named (needs tic, and the font files make font-files and make cullet-mono stage)
	install -Dm755 $(VTIUM_MINSPEC) $(DESTDIR)/usr/bin/vtium
	$(call check-relr,$(DESTDIR)/usr/bin/vtium)
	$(call check-glibc,$(DESTDIR)/usr/bin/vtium)
	install -Dm755 $(CONFIG_RELEASE) $(DESTDIR)/usr/bin/vtium-config
	$(call check-glibc,$(DESTDIR)/usr/bin/vtium-config)
	install -Dm755 $(CLIPBOARD_RELEASE) $(DESTDIR)/usr/bin/vtium-clipboard
	$(call check-glibc,$(DESTDIR)/usr/bin/vtium-clipboard)
	ln -sfn vtium-clipboard $(DESTDIR)/usr/bin/vticlip
	tools/vt/units.sh --name vtium --exec /usr/bin/vtium --by 'make install-tree' --conflicts 'kmsconvt@%i.service'$(if $(PAM_SESSION), --seat) $(DESTDIR)/usr/lib/systemd/system
	install -dm755 $(DESTDIR)/usr/share/terminfo
	tic -x -o $(DESTDIR)/usr/share/terminfo $(VTIUM_INSTALL)/xterm-vtium.ti
	tic -x -o $(DESTDIR)/usr/share/terminfo $(VTIUM_INSTALL)/kmscon.ti
	install -Dm644 $(VTIUM_INSTALL)/vtium.conf.example $(DESTDIR)/etc/vtium/vtium.conf.example
	$(call install_frits,$(frit_files),$(call layer_paths,$(FRIT_LAYERS)))
	install -Dm644 -t $(DESTDIR)/usr/share/doc/vtium/examples $(VTIUM_INSTALL)/examples/71-vtium-local.rules $(VTIUM_INSTALL)/examples/vtium.hwdb
	install -Dm644 $(VTIUM_INSTALL)/pointers.svg $(DESTDIR)/usr/share/vtium/pointers.svg
	$(call install-font-files,$(DESTDIR)/usr/share/vtium/fonts,$(DESTDIR)/usr/share/licenses/vtium)
	$(call install-package-data,$(DESTDIR)/usr/share/vtium,$(DESTDIR)/usr/share/licenses/vtium)
	install -Dm644 -t $(DESTDIR)/usr/share/licenses/vtium $(LICENSE_FILES)
	$(call install-man,$(DESTDIR)/usr/share/man)
ifneq ($(IMAGES),)
	install -Dm755 $(IMAGE_RELEASE) $(DESTDIR)/usr/bin/vtium-image
	$(call check-glibc,$(DESTDIR)/usr/bin/vtium-image)
	install -Dm644 $(VTIUM_INSTALL)/vtium-image.conf $(DESTDIR)/usr/lib/sysusers.d/vtium-image.conf
	install -Dm755 $(GALLERY) $(DESTDIR)/usr/bin/vtium-gallery
	install -Dm644 $(MAN_DIR)/vtium-gallery.1 $(DESTDIR)/usr/share/man/man1/vtium-gallery.1
endif
ifneq ($(PAM_SESSION),)
	install -Dm644 $(VTIUM_INSTALL)/vtium.pam $(DESTDIR)/etc/pam.d/vtium
endif

# vtium as a package ships it, staged under DESTDIR with PREFIX, so that a
# package's build can be one call: make install DESTDIR="$pkgdir"
# PREFIX=/usr PKGNAME="$pkgname" (ADR-1500 D1, D3). packaging/arch's AUR
# recipes call this target and install-terminfo, and the vtium-fonts
# recipe calls install-fonts. make install stages the console, built with the
# minspec profile and FEATURES and made to start its glyph helper at
# PREFIX/bin/vtium-config, the helper, the clipboard program
# PREFIX/bin/vtium-clipboard and its short name PREFIX/bin/vticlip, a link to
# it (ADR-1007 D1), with images in FEATURES the image helper, which the
# console starts at PREFIX/bin/vtium-image, its sysusers file in
# PREFIX/lib/sysusers.d, which creates the helper's account for the login
# prompt (ADR-1101 D1, D2), and the picture gallery tool as
# PREFIX/bin/vtium-gallery (ADR-1500 D8), the VT unit vtium@.service and its
# getty fallback vtium-getty@.service, generated by tools/vt/units.sh, the
# example configuration /etc/vtium/vtium.conf.example, the font sets FRITS
# names in their category folders under /etc/vtium/fonts and the layers
# FRIT_LAYERS names in /etc/vtium/fonts/layers, all of which stay in /etc
# whatever PREFIX is, because vtium reads its configuration and its font
# sets from /etc/vtium alone (ADR-1204 D9, ADR-1500 D3, D4, D8), the example
# udev rules and hwdb fragment as documentation,
# LICENSE_FILES under PKGNAME's name, the man pages in PREFIX/share/man, the
# color schemes and preset vtlooks in PREFIX/share/vtium with the schemes'
# license texts in PKGNAME's license folder, and with libseat and logind in
# FEATURES seat mode's unit vtium-seat@.service and the PAM file
# /etc/pam.d/vtium (ADR-1400 D3).
# HWACCEL and DISPLAY_BUFFERS set the defaults of hwaccel and
# display-buffers that both builds read from the key table (ADR-1500 D8).
# make install-terminfo stages vtium's terminfo entry, compiled under its
# name xterm-vtium and its alias vtium, with LICENSE and COPYING, for the
# package that holds the terminfo entry alone. Neither target stages a file
# an installed kmscon's packages own: nothing under /etc/kmscon, no terminfo
# entry named kmscon and no kmsconvt@ unit. The builds use the host's
# toolchain, as a distribution's package builds do, in a target directory
# of their own, INSTALL_TARGET_DIR.
# CARGO_BUILD_FLAGS goes to each cargo call, as --frozen for a package build
# that fetched its crates first.
PREFIX := /usr/local
PKGNAME := vtium
# The license files make install and make install-tree stage in the
# package's license directory: vtium's LICENSE, COPYING, which holds the
# notices for the third-party code and data vtium builds in, and the two
# license texts COPYING points to, the SIL Open Font License of the Unifont
# font and the license of X.Org's color names. make install-terminfo stages
# LICENSE and COPYING alone, because the terminfo entry holds neither the
# font nor the color names. tiny-skia's BSD-3-Clause notice goes with the
# font helper, which draws SVG pointer files with it (ADR-1309).
LICENSE_FILES := LICENSE COPYING crates/vtium-font/data/OFL-1.1.txt crates/vtium-term/gen-color-names/src/rgb-license.txt crates/vtium-config/licenses/tiny-skia-LICENSE
INSTALL_TARGET_DIR := $(or $(CARGO_TARGET_DIR),target)/install
CARGO_BUILD_FLAGS :=
# make install stages the package's font files, color schemes, preset
# vtlooks and pointer file under PREFIX/share/vtium, so make install-build builds the console
# and vtium-config to read them from there, and the list of a fresh install
# names the folder the fonts really are in. At PREFIX=/usr these are the
# folders both read when the variables are unset.
package_folders = VTIUM_PACKAGE_FONTS=$(PREFIX)/share/vtium/fonts VTIUM_PACKAGE_PALETTES=$(PREFIX)/share/vtium/palettes VTIUM_PACKAGE_LOOKS=$(PREFIX)/share/vtium/looks VTIUM_PACKAGE_POINTERS=$(PREFIX)/share/vtium/pointers.svg
UNITDIR := $(PREFIX)/lib/systemd/system
TERMINFO_SRC := $(VTIUM_INSTALL)/xterm-vtium.ti
# The build settings a package names for two defaults (ADR-1500 D8):
# HWACCEL, on or off, and DISPLAY_BUFFERS, 1, 2 or 3. Each target that
# builds the console or a helper hands each setting named to its builds as
# VTIUM_HWACCEL and VTIUM_DISPLAY_BUFFERS. Those targets are make
# install-build, make minspec, make vt-dev-console, make config-release
# and make image-release. The review targets, make vt-install, make
# install-tree and the tier 3 runs build through them, so their builds
# take the settings too. The key table makes each setting the key's
# default. Unnamed, the key table's own defaults stand.
#
# make reads a setting named on its command line, and one exported in the
# shell. An exported HWACCEL or DISPLAY_BUFFERS therefore reaches tier 3's
# install tree as well. make checks each value for every target that
# builds.
#
# A review target or make vt-install given a binary with VTIUM_BIN builds
# no console. It refuses either setting named on make's command line,
# because it could not give that setting to the binary.
build_settings = $(if $(HWACCEL),VTIUM_HWACCEL=$(HWACCEL) )$(if $(DISPLAY_BUFFERS),VTIUM_DISPLAY_BUFFERS=$(DISPLAY_BUFFERS) )
SETTINGS_GOALS := install install-build install-tree minspec vt-dev-console config-release image-release $(VT_GOALS) vt-install tier3 tier3-minimal tier3-images tier3-nosystemd
ifneq ($(filter $(SETTINGS_GOALS),$(MAKECMDGOALS)),)
ifneq ($(and $(HWACCEL),$(filter-out on off,$(HWACCEL))$(word 2,$(HWACCEL))),)
$(error HWACCEL is on or off, not '$(HWACCEL)')
endif
ifneq ($(and $(DISPLAY_BUFFERS),$(filter-out 1 2 3,$(DISPLAY_BUFFERS))$(word 2,$(DISPLAY_BUFFERS))),)
$(error DISPLAY_BUFFERS is 1, 2 or 3, not '$(DISPLAY_BUFFERS)')
endif
endif
# The console a review target or make vt-install builds, or none when
# VTIUM_BIN names one already built.
vt_console_build := $(strip $(vt_build) $(if $(filter vt-install,$(MAKECMDGOALS)),$(filter $(VTIUM_DEV),$(VTIUM_BIN))))
ifneq ($(filter $(VT_GOALS) vt-install,$(MAKECMDGOALS)),)
ifeq ($(vt_console_build),)
ifneq ($(strip $(filter command line,$(origin HWACCEL)) $(filter command line,$(origin DISPLAY_BUFFERS))),)
$(error $(firstword $(filter $(VT_GOALS) vt-install,$(MAKECMDGOALS))): HWACCEL and DISPLAY_BUFFERS set the defaults a build bakes into the console, and VTIUM_BIN=$(VTIUM_BIN) is built already; leave them out, or leave VTIUM_BIN out so the console is built with them)
endif
endif
endif
# Each of these targets stages under DESTDIR alone: DESTDIR is not empty
# and not the machine's own root, so none of them writes a file over the
# packaged vtium's or beside it in /usr/local. make install, make
# install-terminfo and make install-fonts also need DESTDIR named, while
# make install-tree stages under its own default, target/install-tree.
STAGE_GOALS := install install-terminfo install-fonts install-tree
stage_goal = $(firstword $(filter $(STAGE_GOALS),$(MAKECMDGOALS)))
ifneq ($(filter $(STAGE_GOALS),$(MAKECMDGOALS)),)
ifneq ($(filter install install-terminfo install-fonts,$(MAKECMDGOALS)),)
ifeq ($(origin DESTDIR),file)
$(error $(firstword $(filter install install-terminfo install-fonts,$(MAKECMDGOALS))): name DESTDIR=DIR, the directory it stages the files under; it never installs onto this machine)
endif
endif
ifeq ($(strip $(DESTDIR)),)
$(error $(stage_goal): DESTDIR is empty, which would install onto this machine; name the directory it stages the files under)
endif
ifeq ($(shell readlink -m -- '$(DESTDIR)'),/)
$(error $(stage_goal): DESTDIR=$(DESTDIR) is this machine's root; name the directory it stages the files under)
endif
endif
# make install writes PREFIX into the font sets with sed, where &, | and \
# have a meaning of their own, so it refuses a PREFIX that holds one.
ifneq ($(filter install,$(MAKECMDGOALS)),)
ifneq ($(findstring &,$(PREFIX))$(findstring |,$(PREFIX))$(findstring \,$(PREFIX)),)
$(error install: PREFIX=$(PREFIX) holds &, | or \, which make install cannot write into the font sets; name a PREFIX without them)
endif
endif

.PHONY: install-build
install-build: ## Build what make install stages, for PREFIX (default /usr/local), with FEATURES='...', HWACCEL=on|off, DISPLAY_BUFFERS=1|2|3 and CARGO_BUILD_FLAGS='...' when named
	$(build_settings)$(package_folders) VTIUM_FONT_HELPER=$(PREFIX)/bin/vtium-config $(if $(IMAGES),VTIUM_IMAGE=$(PREFIX)/bin/vtium-image) VTIUM_REVISION=$(or $(shell git describe --always --dirty 2>/dev/null),unknown) cargo rustc $(CARGO_BUILD_FLAGS) --profile minspec -p vtium --bin vtium --target-dir $(INSTALL_TARGET_DIR) $(if $(strip $(FEATURES)),--features '$(strip $(FEATURES))') -- $(RELR_LINK)
	$(call check-relr,$(INSTALL_TARGET_DIR)/minspec/vtium)
	grep -qaF '$(PREFIX)/bin/vtium-config' $(INSTALL_TARGET_DIR)/minspec/vtium || { echo "$(INSTALL_TARGET_DIR)/minspec/vtium does not start its helper at $(PREFIX)/bin/vtium-config" >&2; exit 1; }
# At PREFIX=/usr this path is also the one vtium is built with when
# VTIUM_IMAGE is unset, so there the check passes whether or not VTIUM_IMAGE
# reached the build, and the console is right either way. It still catches
# a console left from a build for another prefix, which lacks the path.
ifneq ($(IMAGES),)
	grep -qaF '$(PREFIX)/bin/vtium-image' $(INSTALL_TARGET_DIR)/minspec/vtium || { echo "$(INSTALL_TARGET_DIR)/minspec/vtium does not start its image helper at $(PREFIX)/bin/vtium-image" >&2; exit 1; }
endif
	$(build_settings)$(package_folders) cargo build $(CARGO_BUILD_FLAGS) --release -p vtium-config -p vtium-clipboard $(if $(IMAGES),-p vtium-image-helper) --target-dir $(INSTALL_TARGET_DIR)

.PHONY: install
install: font-files-check install-build ## Stage what the vtium package ships under DESTDIR=DIR, for PREFIX (default /usr/local), PKGNAME (default vtium) and FRITS='...' (default terminus cullet-mono jetbrains-mono unifont-icons unscii-icons), with the font files make font-files and make cullet-mono stage: make install DESTDIR=DIR PREFIX=/usr
	install -Dm755 $(INSTALL_TARGET_DIR)/minspec/vtium $(DESTDIR)$(PREFIX)/bin/vtium
	install -Dm755 $(INSTALL_TARGET_DIR)/release/vtium-config $(DESTDIR)$(PREFIX)/bin/vtium-config
	install -Dm755 $(INSTALL_TARGET_DIR)/release/vtium-clipboard $(DESTDIR)$(PREFIX)/bin/vtium-clipboard
	ln -sfn vtium-clipboard $(DESTDIR)$(PREFIX)/bin/vticlip
ifneq ($(IMAGES),)
	install -Dm755 $(INSTALL_TARGET_DIR)/release/vtium-image $(DESTDIR)$(PREFIX)/bin/vtium-image
	install -Dm644 $(VTIUM_INSTALL)/vtium-image.conf $(DESTDIR)$(PREFIX)/lib/sysusers.d/vtium-image.conf
	install -Dm755 $(GALLERY) $(DESTDIR)$(PREFIX)/bin/vtium-gallery
	install -Dm644 $(MAN_DIR)/vtium-gallery.1 $(DESTDIR)$(PREFIX)/share/man/man1/vtium-gallery.1
endif
	$(call install-man,$(DESTDIR)$(PREFIX)/share/man)
	tools/vt/units.sh --name vtium --exec $(PREFIX)/bin/vtium --by 'make install' --conflicts 'kmsconvt@%i.service'$(if $(PAM_SESSION), --seat) $(DESTDIR)$(UNITDIR)
ifneq ($(PAM_SESSION),)
	install -Dm644 $(VTIUM_INSTALL)/vtium.pam $(DESTDIR)/etc/pam.d/vtium
endif
	install -Dm644 $(VTIUM_INSTALL)/vtium.conf.example $(DESTDIR)/etc/vtium/vtium.conf.example
	$(call install_frits,$(frit_files),$(call layer_paths,$(FRIT_LAYERS)))
# The sets and layers name the font files by their path under /usr, so for
# another PREFIX make install rewrites that path in each one it staged. With
# no set and no layer staged there is no folder to rewrite in.
ifneq ($(PREFIX),/usr)
	$(if $(strip $(FRITS) $(FRIT_LAYERS)),find $(DESTDIR)/etc/vtium/fonts -type f \( -name '*.vtfrit' -o -name '*.vtlayer' \) -exec sed -i 's|/usr/share/vtium/fonts/|$(PREFIX)/share/vtium/fonts/|g' {} +)
endif
	install -Dm644 -t $(DESTDIR)$(PREFIX)/share/doc/vtium/examples $(VTIUM_INSTALL)/examples/71-vtium-local.rules $(VTIUM_INSTALL)/examples/vtium.hwdb
	install -Dm644 $(VTIUM_INSTALL)/pointers.svg $(DESTDIR)$(PREFIX)/share/vtium/pointers.svg
	$(call install-font-files,$(DESTDIR)$(PREFIX)/share/vtium/fonts,$(DESTDIR)$(PREFIX)/share/licenses/$(PKGNAME))
	$(call install-package-data,$(DESTDIR)$(PREFIX)/share/vtium,$(DESTDIR)$(PREFIX)/share/licenses/$(PKGNAME))
	install -Dm644 -t $(DESTDIR)$(PREFIX)/share/licenses/$(PKGNAME) $(LICENSE_FILES)

.PHONY: install-fonts
install-fonts: ## Stage the font sets and layers the vtium-fonts package ships, FONTS_FRITS='...' and FONTS_LAYERS='...' (default the catalog but the sets and layers a console package ships), in /etc/vtium/fonts under DESTDIR=DIR
	$(call install_frits,$(call frit_paths,$(FONTS_FRITS)),$(call layer_paths,$(FONTS_LAYERS)))

.PHONY: install-terminfo
install-terminfo: ## Stage vtium's terminfo entry, compiled as xterm-vtium and its alias vtium, under DESTDIR=DIR for PREFIX and PKGNAME (default vtium-terminfo) (needs tic)
	@[ -f '$(TERMINFO_SRC)' ] || { echo "install-terminfo: no $(TERMINFO_SRC), the entry it compiles" >&2; exit 1; }
	install -dm755 $(DESTDIR)$(PREFIX)/share/terminfo
	tic -x -e xterm-vtium -o $(DESTDIR)$(PREFIX)/share/terminfo $(TERMINFO_SRC)
	@[ -f '$(DESTDIR)$(PREFIX)/share/terminfo/x/xterm-vtium' ] && [ -f '$(DESTDIR)$(PREFIX)/share/terminfo/v/vtium' ] && [ -z "$$(find '$(DESTDIR)$(PREFIX)/share/terminfo' -type f ! -path '*/x/xterm-vtium' ! -path '*/v/vtium')" ] || { echo "install-terminfo: tic did not compile xterm-vtium and its alias vtium alone" >&2; exit 1; }
	install -Dm644 -t $(DESTDIR)$(PREFIX)/share/licenses/$(if $(filter command line,$(origin PKGNAME)),$(PKGNAME),vtium-terminfo) LICENSE COPYING

# The vendor archive each GitHub release carries: the source of the outside
# crates Cargo.lock names, for a package manager that builds with no network
# (ADR-1500 D9). tools/vendor/archive's header describes the archive, how it
# stays byte-identical for a commit and how the check builds offline.
# VENDOR_VERSION names the version in the file name; without it the script
# takes HEAD's tag, or git describe. The check copies Cullet Mono's faces
# for make install from CULLET_MONO_OUT when make cullet-mono has built them
# there, and builds them itself when it has not.
VENDOR_DIR = $(or $(CARGO_TARGET_DIR),target)/vendor-archive

.PHONY: vendor-archive
vendor-archive: export VTIUM_VENDOR_VERSION = $(value VENDOR_VERSION)
vendor-archive: ## Write the release files for the commit HEAD names into VENDOR_DIR: the source archives vtium-VERSION.tar.gz and .zip from git archive, the vendor archive vtium-VERSION-vendor.tar.zst with the source of every crate Cargo.lock names, its .sha256, the manifest of its crates, vtium-VERSION-dependencies.tsv, and SHA256SUMS over them all (VENDOR_DIR=DIR, VENDOR_VERSION=VERSION)
	tools/vendor/archive pack '$(VENDOR_DIR)'

.PHONY: vendor-archive-check
vendor-archive-check: export VTIUM_VENDOR_VERSION = $(value VENDOR_VERSION)
vendor-archive-check: export VTIUM_VENDOR_CULLET_MONO = $(abspath $(CULLET_MONO_OUT))
vendor-archive-check: vendor-archive ## Write the release files, check them against SHA256SUMS and the source archives against the commit, then build the workspace and make install from a clean copy of HEAD with the archive unpacked over it, offline and --locked, and fail if it does not build or if the manifest does not list the archive's crates (stages the font files first: needs the network once, and FontForge 20251009 unless make cullet-mono has built the faces)
	tools/vendor/archive check '$(VENDOR_DIR)'

# The ARM single-board computers minspec is judged against
# (docs/architecture.md): each target links against a pinned Debian sysroot
# and runs its tests under qemu-user, through tools/arm's wrappers
# (tools/arm/README.md). The sysroots live in a cache every checkout shares.
# The wrappers are named here, for these targets alone, so a native build on
# an ARM machine uses its own toolchain.
ARM_TARGETS      ?= aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf
ARM_TEST_TARGETS ?= aarch64-unknown-linux-gnu
arm arm-test arm-memory: export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = $(ARM_WRAP)/aarch64-unknown-linux-gnu-cc
arm arm-test arm-memory: export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER = $(ARM_WRAP)/aarch64-unknown-linux-gnu-run
arm arm-test arm-memory: export PKG_CONFIG_aarch64_unknown_linux_gnu = $(ARM_WRAP)/aarch64-unknown-linux-gnu-pkg-config
arm arm-test arm-memory: export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER = $(ARM_WRAP)/armv7-unknown-linux-gnueabihf-cc
arm arm-test arm-memory: export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_RUNNER = $(ARM_WRAP)/armv7-unknown-linux-gnueabihf-run
arm arm-test arm-memory: export PKG_CONFIG_armv7_unknown_linux_gnueabihf = $(ARM_WRAP)/armv7-unknown-linux-gnueabihf-pkg-config
# Each target's sysroot directory, which moves when its pins change. The
# pkg-config crate reruns a build script when this changes, so libudev-sys
# never keeps a search path into a sysroot the pins have left. It is
# expanded once, when make reads the line: a recursively expanded variable
# re-expands inside its own $(shell) and overflows make's stack.
arm arm-test arm-memory: export PKG_CONFIG_SYSROOT_DIR_aarch64_unknown_linux_gnu := $(shell tools/arm/sysroot dir arm64)
arm arm-test arm-memory: export PKG_CONFIG_SYSROOT_DIR_armv7_unknown_linux_gnueabihf := $(shell tools/arm/sysroot dir armhf)

.PHONY: arm-sysroot
arm-sysroot: ## Fetch and verify the Debian sysroots the ARM targets link against (VTIUM_ARM_SYSROOTS=DIR)
	tools/arm/sysroot fetch arm64 armhf

# The oldest Rust vtium builds on: the rust-version Cargo.toml declares. The
# build has a target directory of its own, so the gates' other builds keep
# theirs.
MSRV := $(shell sed -n 's/^rust-version = "\(.*\)"$$/\1/p' Cargo.toml)

# make deny, a gate, checks the license and the source of every crate that a
# lock file of vtium's names, and runs cargo-deny's bans check on them, against
# deny.toml, which says which lock files and why. make advisories checks the
# workspace's crates against the RustSec advisory database, which cargo-deny
# downloads, so it is no gate: CI runs it as the job advisories. Both need
# cargo-deny at DENY_VERSION, the version CI installs, so that the gate judges
# the same way on every machine.
DENY_VERSION   = 0.20.2
DENY_INSTALL   = cargo install --locked cargo-deny --version $(DENY_VERSION)
DENY_MANIFESTS = Cargo.toml tools/shader-timing/Cargo.toml $(sort $(wildcard crates/*/fuzz/Cargo.toml crates/*/gen-*/Cargo.toml))
define deny-version
@v=$$(cargo deny --version 2>/dev/null) || v=; [ "$$v" = "cargo-deny $(DENY_VERSION)" ] || { echo "$@ needs cargo-deny $(DENY_VERSION), and this machine has $${v:-no cargo-deny}. Install it with: $(DENY_INSTALL)" >&2; exit 1; }
endef

.PHONY: keymaps
keymaps: ## Compile the keyboard tests' keymap fixtures again from this machine's xkeyboard-config with xkbcli, and record its version (needs xkbcli)
	tools/keymaps/compile

.PHONY: deny
deny: ## Check the license and the source of every crate that a lock file of vtium's names against deny.toml (needs cargo-deny at DENY_VERSION)
	$(deny-version)
	@status=0; for m in $(DENY_MANIFESTS); do echo "deny: $$m"; cargo deny --locked --manifest-path "$$m" --workspace check bans licenses sources || status=1; done; exit $$status

.PHONY: advisories
advisories: ## Check the workspace's crates against the RustSec advisory database, which cargo-deny downloads (needs the network and cargo-deny at DENY_VERSION)
	$(deny-version)
	cargo deny --locked check advisories

.PHONY: msrv
msrv: ## Build the workspace, its tests and the minspec profile with the oldest Rust vtium builds on, Cargo.toml's rust-version, in target/msrv (rustup installs that Rust when it is missing)
	@[ -n "$(MSRV)" ] || { echo "msrv: Cargo.toml declares no rust-version" >&2; exit 1; }
	@rustup run $(MSRV) rustc --version >/dev/null 2>&1 || rustup toolchain install $(MSRV) --profile minimal --no-self-update
	cargo +$(MSRV) build --workspace --all-features --all-targets --locked --target-dir target/msrv
	cargo +$(MSRV) build --profile minspec --locked --target-dir target/msrv

.PHONY: arm
arm: arm-sysroot ## Build the minspec profile for each ARM target, target/TRIPLE/minspec (ARM_TARGETS='...'; needs clang)
	VTIUM_REVISION=$(or $(shell git describe --always --dirty 2>/dev/null),unknown) cargo build --profile minspec $(REMAP_PATHS) $(addprefix --target ,$(ARM_TARGETS))
	for t in $(ARM_TARGETS); do \
		VTIUM_REVISION=$(or $(shell git describe --always --dirty 2>/dev/null),unknown) cargo rustc --profile minspec $(REMAP_PATHS) -p vtium --bin vtium --target $$t -- $(RELR_LINK) $(ICF_LINK) || exit 1; \
		$(call check-relr,$(or $(CARGO_TARGET_DIR),target)/$$t/minspec/vtium) || exit 1; \
		$(call check-paths,$(or $(CARGO_TARGET_DIR),target)/$$t/minspec/vtium) || exit 1; \
		$(call check-glibc,$(or $(CARGO_TARGET_DIR),target)/$$t/minspec/vtium) || exit 1; \
	done

.PHONY: arm-test
arm-test: arm-sysroot ## Run the workspace's tests for each ARM target under qemu-user (ARM_TEST_TARGETS='...'; needs clang, qemu-user)
	cargo test --workspace --all-features --no-fail-fast $(addprefix --target ,$(ARM_TEST_TARGETS))

# vtium-term's memory bounds on a 32-bit target, a gate: tests/memory.rs pins
# the 32-bit figures beside the 64-bit ones, so a change that moves either
# fails on its own target. aarch64 runs the whole suite natively in CI's
# arm-native job. Only the library's tests and tests/memory.rs run, in a
# release build, because vtium-term's whole debug suite under qemu-user takes
# more than ten minutes. They run once without images and once with every
# feature, and the second run goes ahead when the first fails.
.PHONY: arm-memory
arm-memory: arm-sysroot ## Run vtium-term's library tests and tests/memory.rs for armv7 under qemu-user, in a release build, without images and with every feature (needs clang, qemu-user)
	@status=0; for features in '' --all-features; do \
		echo "arm-memory: vtium-term $${features:-without images}"; \
		cargo test --release -p vtium-term --target armv7-unknown-linux-gnueabihf $$features --no-fail-fast --lib --test memory || status=1; \
	done; exit $$status

# vtium-term's standing write benchmarks (crates/vtium-term/examples/
# write_bench.rs): the CPU time the terminal takes to interpret a full-screen
# redraw, a scroll flood, a run of clusters and plain text, with no renderer.
# It is no gate. BENCH='redraw cat' picks benchmarks, BENCH_REPS sets the
# repetitions, and BENCH_FEATURES passes features, for example images.
BENCH          ?=
BENCH_REPS     ?= 15
BENCH_FEATURES ?=
.PHONY: bench
bench: ## Time vtium-term's write benchmarks: ms a frame and a MiB, median with the fastest and slowest of BENCH_REPS runs (BENCH='redraw tui scroll clusters cat')
	cargo run --release -p vtium-term --example write_bench $(if $(BENCH_FEATURES),--features $(BENCH_FEATURES)) -- --reps $(BENCH_REPS) $(BENCH)

# vtium-render's drawing benchmark (crates/vtium-render/examples/
# draw_bench.rs): the CPU time the renderer takes to redraw a whole screen of
# text into memory, with font-ligatures off and on. It is no gate.
# DRAW_BENCH='text spaced' picks benchmarks, and BENCH_REPS sets the
# repetitions.
DRAW_BENCH ?=
.PHONY: draw-bench
draw-bench: ## Time a full CPU redraw of a 240×67 screen of text, with font-ligatures off and on, and of underlined text, upright and turned (DRAW_BENCH='text equals arrows spaced underlined')
	cargo run --release -p vtium-render --features bidi --example draw_bench -- --reps $(BENCH_REPS) $(DRAW_BENCH)

# Tier 1 and the capability probes, against the goldens and probes in
# tools/conformance/.
.PHONY: conformance
conformance: ## Judge vtium against tier 1's goldens
	cargo build --release -p vtium-conformance --features images,bidi --bins
	$(or $(CARGO_TARGET_DIR),target)/release/conformance run

.PHONY: conformance-probes
conformance-probes: ## Run the capability probes through vtium-replay
	cargo build --release -p vtium-conformance --features images,bidi --bins
	$(or $(CARGO_TARGET_DIR),target)/release/conformance probes

# Records real programs through a pty with this machine's bash, less, vim
# and tmux; not a gate.
.PHONY: conformance-record
conformance-record: ## Record the corpus's programs into OUT=DIR, each <name>.bin and <name>.cuts (NAMES='prog-shell ...', default all)
	$(if $(OUT),,$(error conformance-record needs OUT=DIR))
	cargo build --release -p vtium-conformance --features record --bin conformance-record
	$(or $(CARGO_TARGET_DIR),target)/release/conformance-record $(OUT) $(NAMES)

.PHONY: conformance-issues
conformance-issues: ## Check ADR-1000's issue column and the capability list against GitHub's issue states (needs gh)
	cargo build --release -p vtium-conformance --bins
	$(or $(CARGO_TARGET_DIR),target)/release/conformance issues

.PHONY: test-tools
# shellcheck gates at warning: its info notes differ between versions, and
# the scripts' `A && B || die` is deliberate.
test-tools: ## Test the VT review tools, the tier 3 harness, the gallery tool, the tier 1 pictures tool's selection, naming, manifest and match check and its live path's parity with the pictures in a pty, the fuzz runner, the gate runner, the fork-refs check, the prose check, the docs' claims check, the sysroot fetch, the stage check, the vendor archive's manifest and the release files' check, the font files' staging and check, and the Cullet Mono build's checks, the shader timing harness on llvmpipe without a VT, sudo, logind, a VM or nightly (shellcheck when installed), the set writer without its deflate, the key table's crates built as a package names HWACCEL and DISPLAY_BUFFERS, that the console leaves the deflate out, and that make gates runs AGENTS.md's gate line
	python3 -I tools/vt/tests/test_vt.py
	python3 -I tools/vt/tests/test_accept.py
	python3 -I tools/vt/tests/test_typepad.py
	python3 -I -B tools/vt/tests/test_vim_vt510.py
	python3 -I -B tools/tier3/test_t3img.py
	python3 -I -B tools/tier3/test_t3.py
	python3 -I -B tools/gallery/tests/test_gallery.py
	python3 -I -B tools/fuzz/test_run.py
	python3 -I -B tools/gates/test_run.py
	python3 -I -B docs/scripts/test_fork_refs.py
	python3 -I -B docs/scripts/test_prose.py
	python3 -I -B docs/scripts/test_claims.py
	python3 -I -B docs/scripts/test_coverage.py
	python3 -I -B tools/arm/test_sysroot.py
	python3 -I -B tools/stage/test_check.py
	python3 -I -B tools/vendor/test_manifest.py
	python3 -I -B tools/vendor/test_archive.py
	python3 -I -B tools/fonts/test_files.py
	cargo test -q -p vtium-font --test set
	cargo test -q -p vtium-tier1-shots --features draw
	cargo test -q -p vtium-site
	cargo test -q --manifest-path tools/cullet-mono/Cargo.toml --target-dir $(or $(CARGO_TARGET_DIR),target)/cullet-mono-tool
	cargo test -q --locked --manifest-path tools/shader-timing/Cargo.toml --target-dir $(or $(CARGO_TARGET_DIR),target)/shader-timing-tool
	$(TEST_BUILD_SETTINGS)
	@if cargo tree -p vtium --target $(X86_TARGET) -e no-dev,features -i vtium-font | grep -q 'feature "deflate"'; then echo "the console enables vtium-font's deflate feature, which only vtium-config takes (ADR-1200 D19)" >&2; exit 1; else echo "the console links no deflate"; fi
	@$(check-gate-line)
	@if command -v shellcheck >/dev/null; then shellcheck -S warning -x tools/vt/*.sh tools/vt/checks/*.sh tools/vt/sections/*.sh tools/tier3/fetch-image.sh tools/tier3/fetch-debian.sh tools/tier3/showcase.sh tools/gallery/gallery tools/tier3/guest/setup tools/tier3/guest/t3 tools/tier3/guest/t3-agent tools/tier3/guest/t3-novt tools/tier3/guest/tier3-install tools/tier3/guest-sysvinit/setup tools/tier3/guest-sysvinit/tier3-install tools/tier3/guest-sysvinit/t3-console tools/arm/sysroot tools/arm/wrap tools/vendor/archive tools/fonts/files && echo "shellcheck: clean at warning"; else echo "shellcheck: not installed, skipped"; fi

# The key table's crates' unit tests, built as the vtium package builds
# them, with VTIUM_HWACCEL=on, VTIUM_DISPLAY_BUFFERS=3 (ADR-1500 D8) and
# vtium's gpu feature, in a target directory of their own so the other
# builds keep their cache. --lib runs the unit tests alone and leaves out
# every integration test.
TEST_BUILD_SETTINGS = VTIUM_HWACCEL=on VTIUM_DISPLAY_BUFFERS=3 cargo test -q --target-dir $(or $(CARGO_TARGET_DIR),target)/build-settings -p vtium-settings -p vtium -p vtium-config --features vtium/gpu --lib

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
# CONF is make vt-login's, make vt-shell's and make vt-check's.
$(VT_GOALS): export VTIUM_VT_CONF =
# RENDER_PATH is make vt-accept's; the others run vtium as DEMO_ARGS says.
$(VT_GOALS): export VTIUM_VT_RENDER_PATH =

.PHONY: vt-accept
vt-accept: export VTIUM_VT_SESSION = accept
vt-accept: export VTIUM_VT_RENDER_PATH = $(value RENDER_PATH)
vt-accept: export VTIUM_VT_PAGES = $(value PAGES)
vt-accept: $(vt_build) $(vt_helper_build) $(vt_image_build) ## Run the acceptance pass on the development VT or a free VT, with pictures and bidi unless FEATURES is named, on the CPU path or the GPU path, every page answered and the pass recorded, then print the pass; PAGES walks only those pages, and the record says the pass is not a full pass (RENDER_PATH=cpu|gpu, PAGES='51 56-glyph-fit', VT=N, FEATURES='...', DEMO_ARGS='...', DRY_RUN=1, VT_DEBUG=1)
	tools/vt/vt.sh

.PHONY: vt-login
vt-login: export VTIUM_VT_SESSION = login
vt-login: export VTIUM_VT_CONF = $(value CONF)
vt-login: $(vt_build) $(vt_helper_build) $(vt_image_build) ## Run vtium with a login prompt on the development VT or a free VT, until the first logout, with the image helper when FEATURES has images and CONF in place of the review's vtium.conf when named (VT=N, CONF=PATH, FEATURES='...', DEMO_ARGS='...', VT_DEBUG=1)
	tools/vt/vt.sh

.PHONY: vt-shell
vt-shell: export VTIUM_VT_SESSION = shell
vt-shell: export VTIUM_VT_CONF = $(value CONF)
vt-shell: $(vt_build) $(vt_helper_build) $(vt_image_build) ## Run your login shell, as you, in vtium on the development VT or a free VT; exit it to end the run; with the image helper when FEATURES has images and CONF in place of the review's vtium.conf when named (VT=N, CONF=PATH, FEATURES='...', DEMO_ARGS='...', VT_DEBUG=1)
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
vt-check: export VTIUM_VT_CONF = $(value CONF)
vt-check: $(vt_build) $(vt_helper_build) $(vt_image_build) ## Run a guide's checks on the development VT or a free VT, with the image helper when FEATURES has images and CONF in place of the review's vtium.conf when named; the operator answers (GUIDE=NAME, GUIDE_STEPS='2 3', CONF=PATH, FEATURES='...', VT_DEBUG=1)
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
vt-dev-console: ## Build the console make vt-install installs, target/vt-dev/x86_64-unknown-linux-gnu/minspec/vtium, with FEATURES='...', HWACCEL=on|off and DISPLAY_BUFFERS=1|2|3 when named (needs clang)
	tools/arm/sysroot fetch amd64
	$(build_settings)VTIUM_FONT_HELPER=$(VTIUM_DEV_HELPER) $(if $(IMAGES),VTIUM_IMAGE=$(VTIUM_DEV_IMAGE)) VTIUM_REVISION=$(or $(shell git describe --always --dirty 2>/dev/null),unknown) cargo rustc --profile minspec $(REMAP_PATHS) -p vtium --bin vtium --target $(X86_TARGET) --target-dir $(VTIUM_DEV_DIR) $(if $(strip $(FEATURES)),--features '$(strip $(FEATURES))') -- $(RELR_LINK) $(ICF_LINK)
	$(call check-relr,$(VTIUM_DEV))
	$(call check-paths,$(VTIUM_DEV))
	$(call check-glibc,$(VTIUM_DEV))
ifneq ($(IMAGES),)
	grep -qaF '$(VTIUM_DEV_IMAGE)' $(VTIUM_DEV) || { echo "$(VTIUM_DEV) does not start its image helper at $(VTIUM_DEV_IMAGE)" >&2; exit 1; }
	grep -qaF '$(VT_IMAGES_MARKER)' $(VTIUM_DEV) || { echo "$(VTIUM_DEV) is built with images but lacks the line '$(VT_IMAGES_MARKER)' that vt-install tells such a build by (tools/vt/vt-install.sh)" >&2; exit 1; }
else
	! grep -qaF '$(VT_IMAGES_MARKER)' $(VTIUM_DEV) || { echo "$(VTIUM_DEV) is built without images but carries the line '$(VT_IMAGES_MARKER)' that vt-install tells a build with images by (tools/vt/vt-install.sh)" >&2; exit 1; }
endif

.PHONY: vt-install
vt-install: $(vt_build) $(vt_dev_build) ## Install a development build as vtium-dev on VT N in place of its getty, started at every boot (VT=N, DRY_RUN=1, FEATURES='...', HWACCEL=on|off, DISPLAY_BUFFERS=1|2|3)
	@case "$$VTIUM_VT_DRY_RUN" in '') tools/vt/vt-install.sh install "$$VTIUM_VT_NR" ;; 1) tools/vt/vt-install.sh install --dry-run "$$VTIUM_VT_NR" ;; *) echo "DRY_RUN=$$VTIUM_VT_DRY_RUN: 1 or nothing" >&2; exit 1 ;; esac

.PHONY: vt-uninstall
vt-uninstall: ## Undo make vt-install on VT N: vtium-dev off, its getty back, the files it wrote removed (VT=N, DRY_RUN=1)
	@case "$$VTIUM_VT_DRY_RUN" in '') tools/vt/vt-install.sh uninstall "$$VTIUM_VT_NR" ;; 1) tools/vt/vt-install.sh uninstall --dry-run "$$VTIUM_VT_NR" ;; *) echo "DRY_RUN=$$VTIUM_VT_DRY_RUN: 1 or nothing" >&2; exit 1 ;; esac

# Mesa keeps its shader cache in files that every GL program of the user
# shares, in ~/.cache by default, and maps those files into memory. When
# another program rewrites a file that a run has mapped, the run dies with
# SIGBUS, as make tier2 did while other GL runs drew. Each target that
# starts GL therefore keeps a cache of its own in the cargo target
# directory, which only its own runs use; a gate of make gates has a target
# directory of its own, so it has its own cache too. The cache stays between
# runs, so a run compiles only the shaders the last one did not, and cargo
# clean removes it. arm-test runs the sysroot's older Mesa, so its cache
# stays apart from the host Mesa's as well.
MESA_CACHES = $(abspath $(or $(CARGO_TARGET_DIR),target))/mesa-shader-cache
tier2 arm-test test-tools gates: export MESA_SHADER_CACHE_DIR = $(MESA_CACHES)/$@

# Tier 2 against its goldens in tools/tier2/goldens. The report names this checkout's commit, as tier 3's
# does (T3_REV). tier2 runs tier2-fonts first, which fetches the outline fonts the outline screens draw
# with into $XDG_CACHE_HOME/vtium/tier2/fonts, and Adwaita's default cursor file the pointer-file-adwaita
# screen draws into $XDG_CACHE_HOME/vtium/tier2/pointers, and checks each against its SHA-256; once they
# are cached, it fetches nothing.
.PHONY: tier2 tier2-fonts
tier2: tier2-fonts ## Draw tier 2's screens and judge them against their goldens
	cargo build --release -p vtium-tier2 --features gpu --bins
	T2_VTIUM_REV=$(T3_REV) $(or $(CARGO_TARGET_DIR),target)/release/tier2 run

tier2-fonts: ## Fetch tier 2's pinned outline fonts and its pinned Adwaita cursor file into the cache, checking each SHA-256
	cargo build --release -p vtium-tier2 --features gpu --bins
	$(or $(CARGO_TARGET_DIR),target)/release/tier2 fetch-fonts

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

# Cullet Mono, vtium's outline Terminus with Nerd Fonts' icons, built from
# the sources that tools/cullet-mono/pins.tsv pins. Not a gate: it needs
# FontForge, Python and, once, the network, which fetches the sources into
# $XDG_CACHE_HOME/vtium/cullet-mono. The build fails unless each face has
# the SHA-256 that pins.tsv pins. cullet-mono-check checks the faces that
# cullet-mono built, without building them again.
#
# References:
# - ADR-1204 D2: the sources, the name and the icon sets.
CULLET_MONO := cargo run --release --manifest-path tools/cullet-mono/Cargo.toml --target-dir $(or $(CARGO_TARGET_DIR),target)/cullet-mono-tool --
CULLET_MONO_OUT := $(or $(CARGO_TARGET_DIR),target)/cullet-mono
.PHONY: font-files font-files-check
font-files: ## Stage the Terminus and Unifont files every build installs in target/font-files: Terminus 4.49.1's archive fetched once into the cache, and each file checked against tools/fonts/pins.tsv (needs curl and the network once)
	$(FONT_FILES_TOOL) fetch $(FONT_FILES)

font-files-check: ## Check the font files make install stages, in target/font-files and target/cullet-mono, against their pins
	$(FONT_FILES_TOOL) check $(FONT_FILES) $(CULLET_MONO_OUT)

.PHONY: cullet-mono cullet-mono-check
cullet-mono: ## Build Cullet Mono from its pinned sources into target/cullet-mono, and check each face against its pinned SHA-256 (needs FontForge, Python and the network once)
	$(CULLET_MONO) build --out $(CULLET_MONO_OUT)

cullet-mono-check: ## Check the Cullet Mono faces in target/cullet-mono against pins.tsv, without building them again
	$(CULLET_MONO) check $(CULLET_MONO_OUT)

# The shader timing harness, which times spatial shaders off-screen on a
# GPU's render node through EGL, with no DRM master. Not a gate: its numbers
# belong to the machine that runs it. DEVICE picks the GPU: surfaceless (the
# default, the GPU Mesa picks), software (llvmpipe) or a render node such as
# /dev/dri/renderD128. ARGS passes more options, such as ARGS='--frames 7'.
# The rows go to target/shader-timing/DEVICE.tsv as well as to the terminal.
#
# References:
# - ADR-1304 D18, "measuring first": what the harness measures, and why.
SHADER_TIMING := cargo run --release --locked --manifest-path tools/shader-timing/Cargo.toml --target-dir $(or $(CARGO_TARGET_DIR),target)/shader-timing-tool --
SHADER_TIMING_OUT := $(or $(CARGO_TARGET_DIR),target)/shader-timing
.PHONY: shader-timing
shader-timing: ## Time the spatial shaders of ADR-1304 D18 off-screen on DEVICE (surfaceless, software or a render node; default surfaceless) at 1920x1080 and 3840x2160, into target/shader-timing/DEVICE.tsv; ARGS='...' passes more options
	@mkdir -p $(SHADER_TIMING_OUT)
	$(SHADER_TIMING) run --device '$(or $(DEVICE),surfaceless)' --tsv '$(SHADER_TIMING_OUT)/$(notdir $(or $(DEVICE),surfaceless)).tsv' $(ARGS)

# What the maintainer reviews tier 1's goldens on (ADR-1505, V3); not a
# gate. tier1-shots replays every case of the corpus and of vtium's own
# cases, and every behaviour scenario, through vtium-replay's code, checks
# each against its goldens, and draws the terminal through vtium's renderer
# into PNGs with a manifest (tools/conformance/README.md, "Tier 1's
# pictures"). A screen that shows Arabic draws in tier 2's scripts font
# set, which make tier2-fonts fetches. OUT must be new, empty or an earlier run's output.
# tier1-play is each picture's live path: it plays one case or scenario in
# this terminal from the same replay, one screen at a time, so a vtium
# console draws each screen itself.
TIER1_SHOTS := $(or $(OUT),$(or $(CARGO_TARGET_DIR),target)/tier1-shots)
.PHONY: tier1-shots tier1-play
tier1-shots: ## Draw every tier 1 golden as a PNG through vtium's renderer in unifont, and Arabic in tier 2's scripts fonts, with manifest.json, into OUT=DIR (default target/tier1-shots)
	cargo build --release -p vtium-tier1-shots --features draw --bins
	$(or $(CARGO_TARGET_DIR),target)/release/tier1-shots $(TIER1_SHOTS)

tier1-play: ## Play one tier 1 case (CASE=NAME) or scenario (SCENARIO=NAME) in this terminal, one screen at a time from the replay its pictures come from; q stops
	@if [ -n "$(CASE)" ] && [ -z "$(SCENARIO)" ]; then :; elif [ -z "$(CASE)" ] && [ -n "$(SCENARIO)" ]; then :; else echo "make tier1-play takes CASE=NAME or SCENARIO=NAME" >&2; exit 2; fi
	cargo build --release -p vtium-tier1-shots --features draw --bins
	$(or $(CARGO_TARGET_DIR),target)/release/tier1-play $(if $(CASE),case $(CASE),scenario $(SCENARIO))

# What the maintainer reviews vtium-config's TUI screen goldens on
# (crates/vtium-config/README.md, "Reviewing the screens"); neither is a gate.
# tui-shots plays the bytes the golden tests capture through vtium's
# renderer into PNGs; tui-demo opens the TUI on the goldens' fixtures in a
# throwaway home that tools/tui-review/demo removes afterwards.
TUI_SHOTS := $(or $(OUT),$(or $(CARGO_TARGET_DIR),target)/tui-shots)
.PHONY: tui-shots tui-demo
tui-shots: ## Draw every TUI screen golden as a PNG through vtium's renderer in unifont, into OUT=DIR (default target/tui-shots), the provisional ones included and marked
	rm -rf $(TUI_SHOTS)/bytes
	VTIUM_CONFIG_SHOTS=$(abspath $(TUI_SHOTS))/bytes cargo test -p vtium-config --test tui --test tui_flow --test tui_effects --test tui_fonts --test tui_looks --test tui_settings -- each_view_at_80x25_and_100x30 the_flow_view_s_states_at_80x25_and_120x32 the_effects_tab_previews_a_preset_at_80x25_and_100x30 the_fonts_tab_s_screens the_color_schemes_and_the_pointer_themes_at_80x25_and_100x30 a_settings_section_and_a_list_of_values_at_80x25_and_100x30
	cargo build --release -p vtium-tui-review --bin tui-shots
	$(or $(CARGO_TARGET_DIR),target)/release/tui-shots crates/vtium-config/tests/screens $(TUI_SHOTS)/bytes $(TUI_SHOTS)

tui-demo: ## Open vtium-config's TUI in this terminal on the screen goldens' fixtures, in a throwaway home removed afterwards (FIXTURE=draft, the default, or figure)
	cargo build -p vtium-config -p vtium-tui-review --bins
	tools/tui-review/demo $(or $(FIXTURE),draft) $(or $(CARGO_TARGET_DIR),target)/debug/tui-demo $(or $(CARGO_TARGET_DIR),target)/debug/vtium-config

# The documentation site (ADR-1502): Zensical, pinned with hashes in
# tools/site/requirements.txt, installed into a virtual environment under
# target/ and run from there. Its first run needs Python 3.11 or later and the
# network. None of these targets is a gate; CI's docs job runs the same strict
# build.
SITE_VENV := target/site-venv
$(SITE_VENV)/installed: tools/site/requirements.txt
	python3 -m venv $(SITE_VENV)
	$(SITE_VENV)/bin/pip install --quiet --require-hashes -r tools/site/requirements.txt
	touch $@

.PHONY: docs docs-serve showcase showcase-grid
docs: $(SITE_VENV)/installed ## Build the documentation site into target/site in strict mode, which fails on a broken link or anchor
	$(SITE_VENV)/bin/zensical build --strict --clean

docs-serve: $(SITE_VENV)/installed ## Serve the documentation site at http://localhost:8000 and rebuild it as you edit; it checks no links, so run make docs too
	$(SITE_VENV)/bin/zensical serve

showcase: ## Make the showcase screens in docs/showcase (tier 3, QEMU/KVM)
	tools/tier3/showcase.sh

showcase-grid: ## Make docs/showcase/grid.png again from the screens already in docs/showcase, with no VM; make showcase makes it too
	python3 -I -B tools/tier3/t3_showcase.py grid

# The crates' interfaces (ADR-1508 D1): rustdoc for each crate under crates/
# that has a library, on the pinned toolchain, written to target/doc. The
# flags turn every rustdoc warning into an error, a broken intra-doc link
# included, so the build is the check of rustdoc's links, which the site's
# strict build cannot see. make api first removes target/doc, so the
# reference holds only what this build writes, and no page that an earlier
# cargo doc left there. make site-api copies the reference into the built
# site under /api/, and make site builds the whole site that way.
#
# make api is a gate of the gate line. The docs workflow runs make site-api
# after Zensical builds the site, so the published site serves the reference
# under /api/, beside the pages Zensical builds from docs/api/.
API_DOC_DIR = $(or $(CARGO_TARGET_DIR),target)/doc
API_PACKAGES = $(foreach c,$(patsubst crates/%/src/lib.rs,%,$(wildcard crates/*/src/lib.rs)),-p $(c))
API_RUSTDOCFLAGS = -D warnings -D rustdoc::broken_intra_doc_links

.PHONY: api site-api site release-notes
api: ## Build the crates' interfaces with rustdoc into target/doc, failing on any rustdoc warning or broken intra-doc link
	rm -rf $(API_DOC_DIR)
	RUSTDOCFLAGS="$(API_RUSTDOCFLAGS)" cargo doc --no-deps --all-features --lib $(API_PACKAGES)

site-api: api ## Copy the crates' interfaces into the built site under target/site/api (run make docs first)
	@test -f target/site/index.html || { echo "target/site has no built site: run make docs first" >&2; exit 1; }
	mkdir -p target/site/api
	cp -R $(API_DOC_DIR)/. target/site/api/

site: docs ## Build the documentation site with the crates' interfaces under /api/
	$(MAKE) site-api

# A release's notes (ADR-1508 D3): docs/releases/VERSION.md is the one text,
# and this writes the GitHub release's body from it, with each link to a page
# of the site turned into the page's full address. The release's steps pass
# the file to gh release create --notes-file. The file goes to
# target/release-notes/ in the checkout even when CARGO_TARGET_DIR points
# elsewhere, because the release's steps name that path.
release-notes: ## Write the GitHub release body for VERSION=V from docs/releases/V.md to target/release-notes/V.md
	@test -n "$(VERSION)" || { echo "name the release: make release-notes VERSION=0.1.0-alpha.9" >&2; exit 1; }
	cargo run -q -p vtium-site --bin release-notes -- --out target/release-notes/$(VERSION).md $(VERSION)

# vtium's man pages, committed in man/. tools/man generates them in roff:
# vtium.conf(5)'s settings from the key table, each command's options from
# that command's own list, and the prose from tools/man/templates. make man
# writes them, and make man-check, a gate of the gate line, fails when a
# committed page differs from what the generator writes, or when mandoc's
# lint reports a warning or worse on one; a style note passes. make install
# and make install-tree install them in PREFIX/share/man, the gallery's page
# only with images in FEATURES, as its program.
#
# man-check runs the mandoc it finds on PATH. When there is none, as on an
# Arch desktop that keeps man-db, it runs the same lint in docker: it builds
# tools/man/mandoc.Dockerfile once as MANDOC_IMAGE, with Ubuntu 24.04's
# mandoc like CI's, and reuses that image afterwards. The container has no
# network, runs as the invoking user, and mounts only man/, read-only.
# mandoc's exit status passes through docker run, so a warning still fails
# the gate. MANDOC_DOCKER=0 turns the docker fallback off.
MAN_DIR := man
MAN_PAGES := vtium.8 vtium-config.1 vtium.conf.5 vtium-clipboard.1 vtium-gallery.1
MAN_LINT := mandoc -T lint -W warning $(addprefix $(MAN_DIR)/,$(MAN_PAGES))
MANDOC_DOCKER ?= 1
MANDOC_IMAGE := vtium-mandoc:1

.PHONY: man man-check
man: ## Write vtium's man pages into man/ from the key table, the commands' option lists and tools/man's templates
	cargo run -q -p vtium-man -- write $(MAN_DIR)

man-check: ## Fail when a man page in man/ is stale or mandoc -T lint finds a warning or worse in one (needs mandoc, or docker when mandoc is missing)
	cargo run -q -p vtium-man -- check $(MAN_DIR)
	@if command -v mandoc >/dev/null; then \
		echo "man-check: linting the man pages with $$(command -v mandoc)"; \
		$(MAN_LINT); \
	elif [ "$(MANDOC_DOCKER)" != 0 ] && docker info >/dev/null 2>&1; then \
		echo "man-check: mandoc is not installed, so man-check lints the man pages in the docker image $(MANDOC_IMAGE)"; \
		docker image inspect $(MANDOC_IMAGE) >/dev/null 2>&1 \
			|| docker build -t $(MANDOC_IMAGE) - < tools/man/mandoc.Dockerfile \
			|| { echo "man-check: cannot build the docker image $(MANDOC_IMAGE) from tools/man/mandoc.Dockerfile." >&2; exit 1; }; \
		docker run --rm --network=none --user "$$(id -u):$$(id -g)" \
			-v "$(CURDIR)/$(MAN_DIR):/vtium/$(MAN_DIR):ro" -w /vtium \
			$(MANDOC_IMAGE) $(MAN_LINT); \
	else \
		echo "man-check: mandoc is not installed and docker is not available, so man-check cannot lint the man pages. Install mandoc, or make docker available." >&2; \
		exit 1; \
	fi

# Tier 3: vtium's install tree in a QEMU/KVM guest with a real kernel VT and
# KMS device, its screens captured and compared with the goldens in
# tools/tier3/goldens (tools/tier3/README.md). QEMU runs headless as the invoking user. Each
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
tier3: tier3-image ## Tier 3: build the install tree, then run the five default variants (2d-1280x800, 2d-7680x2160, 2d-1366x768-potato, 2d-7680x2160-font32, gl-1280x800), or VARIANTS="..." (JOBS=N, FEATURES='...')
	$(T3_RUN) --tree-target install-tree $(VARIANTS)

.PHONY: tier3-minimal
tier3-minimal: tier3-image ## Tier 3 on the minspec machine, 1 vCPU and 512 MB: unifont, a PSF font, the 320x240 and 640x480 displays, vtium with nothing installed, the default font sets at 16, 24 and 40 pixels, and vtium-config apply as root
	$(T3_RUN) --tree-target install-tree $(or $(VARIANTS),2d-potato-min 2d-potato-min-psf 2d-320x240-psf8 2d-640x480-psf16 2d-640x480-frit 2d-640x480-frit-held default-font-16 default-font-24 default-font-40 cullet-mono-16 cullet-mono-24 cullet-mono-40 builtin-font apply-root)

# vtium's pictures on tier 3: the tree's console built with images, and with
# any other features FEATURES names, then the images variants, which draw
# sixel and kitty pictures and pictures the image helper decodes, and check
# the helper's account before login.
.PHONY: tier3-images
tier3-images: tier3-image ## Tier 3's pictures: the install tree built with images, then the images variants on the minspec machine at 640x480
	$(T3_RUN) --features '$(sort images $(subst $(comma), ,$(FEATURES)))' --tree-target install-tree $(or $(VARIANTS),images-640x480 images-640x480-nobody)

# The checks of ADR-1400 D4 (tools/tier3/README.md, "The checks of the
# privilege split"): the probe in tools/tier3/probes, run in the guest as root
# and as a system account. SEATD_SRC=DIR, an unpacked seatd source tree, adds
# check 3's run with a libseat built with its builtin backend.
.PHONY: tier3-rootchecks
tier3-rootchecks: tier3-image ## Measure ADR-1400 D4's checks in the tier 3 guest, answers in target/tier3/rootchecks/answers.txt (SEATD_SRC=DIR for the builtin backend)
	$(T3_PY) $(T3_DIR)/t3_rootchecks.py $(if $(SEATD_SRC),--seatd-source $(SEATD_SRC))

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
		--deadline $(T3_DEADLINE) $(T3_FEATURES) --tree-target install-tree $(or $(VARIANTS),sysvinit-640x480 sysvinit-two-vts sysvinit-no-udev sysvinit-reboot sysvinit-restart sysvinit-power-off busybox-power-off)

# AGENTS.md's gate line, one gate per line, in its order; make test-tools
# fails when the two differ (check-gate-line).
define GATE_LINE
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo test --workspace --all-features
cargo test --workspace
cargo build --profile minspec
make msrv
make deny
docs/scripts/adr lint --check
docs/scripts/adr cite --check
docs/scripts/issue-refs
docs/scripts/fork-refs
docs/scripts/claims
docs/scripts/prose --check
docs/scripts/coverage
make api
make man-check
make test-tools
make conformance
make conformance-probes
make conformance-issues
make tier2
make arm
make arm-memory
endef
export GATE_LINE

# check-gate-line reads the first fenced sh block under "## The gates" in
# AGENTS.md, one gate per line without its `&&` and `\`, and
# fails unless it is GATE_LINE, which reaches awk through the environment.
define check-gate-line
awk 'f && /^```/ { f = 0; done = 1 } f { sub(/^ *(&& )?/, ""); sub(/ \\$$/, ""); got = got $$0 "\n" } /^## The gates$$/ { g = 1 } g && !done && /^```sh$$/ { f = 1 } END { want = ENVIRON["GATE_LINE"] "\n"; if (got == "" || got != want) { printf "make gates runs another gate line than AGENTS.md lists.\nAGENTS.md:\n%sGATE_LINE:\n%s", got, want > "/dev/stderr"; exit 1 } print "make gates runs AGENTS.md'"'"'s gate line" }' AGENTS.md
endef

# make gates runs the gate line's gates through tools/gates/run, up to
# JOBS=N at once, 8 unless JOBS is set on the command line or in the
# environment; the file's own JOBS is tier 3's, 1. Each gate builds in a
# target directory of its own, $(GATES_DIR)/NAME, and writes its output to
# $(GATES_DIR)/logs/NN-NAME.log; the script's docstring describes the run
# and its report. No gate needs the machine to itself. A CARGO_TARGET_DIR
# set by the caller must not be shared by worktrees whose sources differ
# (AGENTS.md, "The gates").
GATES_JOBS = $(if $(filter command line environment,$(origin JOBS)),$(JOBS),8)
GATES_DIR  = $(abspath $(or $(CARGO_TARGET_DIR),target))/gates

.PHONY: gates
gates: ## Run every gate of the gate line, up to JOBS=N at once (default 8), each to the end, report each exit code and fail if any failed
	@python3 -I -B tools/gates/run --jobs $(GATES_JOBS) --dir $(GATES_DIR)

# make ci-local runs the tests in a container with the userland of CI's
# runner, Ubuntu 24.04, so a test that only fails there shows up before a
# push; tools/ci-local/run describes the run. It caches under
# $XDG_CACHE_HOME/vtium/ci-local, never in target/.
BUILDS ?= all-features default

.PHONY: ci-local
ci-local: ## Run cargo test --workspace, with every feature and with the default build, in a container matching CI's Ubuntu 24.04 runner (needs podman or docker; BUILDS='all-features default', TEST_ARGS='-p CRATE ...')
	@tools/ci-local/run --builds '$(BUILDS)' $(TEST_ARGS)

# The steering session runs this before it moves stage to a commit of
# integration (AGENTS.md, "The working loop", step 7). It fetches main from
# origin and fails unless COMMIT descends from it, which happens when a stage
# pull request has merged into main and main has not been merged back into
# integration since.
COMMIT ?= HEAD

.PHONY: stage-check
stage-check: ## Fetch main from origin and fail unless COMMIT (default HEAD) descends from it, before stage moves to COMMIT
	@tools/stage/check '$(COMMIT)'
