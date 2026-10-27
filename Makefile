# vtium's task runner. Each recipe is thin over cargo or a script in tools/;
# `make help` lists the targets. The VT review is described in
# docs/vt-review.md.

.DEFAULT_GOAL := help

# The binary the VT review runs. It runs as root, so it is named in this file
# or on make's command line (make vt-demo VTIUM_BIN=PATH), never taken from a
# stale export in the shell. The default is the minspec build, which the vt
# targets build first; another binary is run as it is.
VTIUM_MINSPEC := target/minspec/vtium
VT_GOALS := vt-demo vt-login vt-check
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
	cargo build --profile minspec -p vtium

.PHONY: test-tools
# shellcheck gates at warning: its info notes differ between versions, and
# the scripts' `A && B || die` is deliberate.
test-tools: ## Test the VT review tools without a VT, sudo or logind (shellcheck when installed)
	python3 -I tools/vt/tests/test_vt.py
	@if command -v shellcheck >/dev/null; then shellcheck -S warning -x tools/vt/*.sh tools/vt/checks/*.sh tools/vt/sections/*.sh && echo "shellcheck: clean at warning"; else echo "shellcheck: not installed, skipped"; fi

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

# Tier 2 against the C reference's goldens at the commit tools/tier2/pin.tsv
# names, read from a kmscon checkout beside this one, and against vtium's
# own. Out of cargo test, which needs no checkout of the C repositories.
KMSCON ?= ../kmscon

.PHONY: tier2
tier2: ## Draw tier 2's screens and judge them against their goldens (KMSCON=DIR)
	cargo build --release -p vtium-tier2 --bins
	target/release/tier2 run --kmscon $(KMSCON)
