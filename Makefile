# fantuan-kernel — the front door for building, running and gating the tree.
#
# This file implements no build step of its own: every target calls the script
# that owns that step (tools/*.sh, tools/kconfig.py), so the scripts and
# docs/BUILD.md stay the single source of truth. What it adds is one place to
# look — the tree mixes Rust, C, NASM, Python and shell, and a fresh clone
# should not have to guess the order.
#
# Configuration: config.mk (tracked defaults) then config.local.mk (your
# overrides, gitignored). `make help` lists everything, `make vars` shows the
# values in effect. Works with GNU make and bmake.

-include config.mk
-include config.local.mk

.PHONY: all help vars doctor \
        build build-riscv build-aarch64 build-i686 build-bios build-all iso \
        run run-graphics run-riscv run-aarch64 run-bios run-i686 \
        config config-text config-check menuconfig \
        check check-i686 check-lines payload \
        smoke test test-matrix \
        apps apps-list sbom \
        clean distclean

# `make` on its own explains itself rather than building something unexpected.
all: help

help:
	@echo "fantuan-kernel — make targets"
	@echo
	@echo "  Setup"
	@echo "    doctor          check the toolchain (Rust targets, QEMU, OVMF, nasm, clang, python3)"
	@echo "    config          edit the kernel configuration (PROFILE=... writes a profile instead)"
	@echo "    config-text     print the effective CONFIG_* values"
	@echo "    config-check    validate depends, do not write"
	@echo "    vars            print the make knobs in effect (ARCH, PROFILE, ...)"
	@echo
	@echo "  Build"
	@echo "    build           build ARCH (default x86_64) with tools/build.sh"
	@echo "    build-riscv | build-aarch64 | build-i686 | build-bios"
	@echo "    build-all       x86_64 + i686 + BIOS chain + hybrid ISO"
	@echo "    iso             hybrid BIOS+UEFI ISO (tools/build-iso.sh)"
	@echo "    check           fast compile check of the workspace (no QEMU, no shell build)"
	@echo "    check-i686      the 32-bit build (nightly + build-std, slower)"
	@echo "    check-lines     the 300-line rule ratchet (tools/line-budget.txt)"
	@echo
	@echo "  Run (QEMU)"
	@echo "    run             boot the built image for ARCH"
	@echo "    run-graphics    x86_64 with the GOP window"
	@echo "    run-riscv | run-aarch64 | run-i686 | run-bios"
	@echo "    (extra flags: make run RUN_ARGS=\"--net --vga std\")"
	@echo
	@echo "  Verify"
	@echo "    smoke           quick suites: $(SMOKE_SET)"
	@echo "    smoke-<name>    one suite, e.g. make smoke-gpl smoke-kms"
	@echo "    test            the full x86 acceptance suite (13 phases, ~25 min)"
	@echo "    test-matrix     across architectures: $(MATRIX_SET)"
	@echo
	@echo "  Apps and licensing"
	@echo "    apps | apps-list   the vendored catalog and its state"
	@echo "    sbom               machine-readable licence inventory"
	@echo
	@echo "  Housekeeping"
	@echo "    clean           remove cargo targets and embedded payloads"
	@echo "    distclean       also remove build/ (logs, images, caches)"
	@echo
	@echo "  Knobs live in config.mk / config.local.mk; docs/BUILD.md explains the"
	@echo "  toolchain, docs/OPERATIONS.md the suites, docs/REPO_POLICY.md the mirrors."

vars:
	@echo "ARCH        = $(ARCH)"
	@echo "PROFILE     = $(PROFILE)"
	@echo "BUILD_ARGS  = $(BUILD_ARGS)"
	@echo "RUN_ARGS    = $(RUN_ARGS)"
	@echo "SMOKE_SET   = $(SMOKE_SET)"
	@echo "MATRIX_SET  = $(MATRIX_SET)"

doctor:
	./tools/check-toolchain.sh

# --- configuration --------------------------------------------------------
# The kernel profile is written by tools/kconfig.py; PROFILE=... makes the
# target non-interactive, otherwise the menu opens.
config:
	@if [ -n "$(PROFILE)" ]; then \
	  python3 tools/kconfig.py --profile $(PROFILE) && python3 tools/kconfig.py --text; \
	else \
	  python3 tools/kconfig.py menu; \
	fi

menuconfig: config

config-text:
	python3 tools/kconfig.py --text

config-check:
	python3 tools/kconfig.py --check

# --- build ----------------------------------------------------------------
# ARCH dispatches to the script that knows the architecture: build.sh covers
# x86_64/riscv64/aarch64, the i686 port has its own entry point.
build:
	@if [ -n "$(PROFILE)" ]; then python3 tools/kconfig.py --profile $(PROFILE) >/dev/null; fi
	@case "$(ARCH)" in \
	  x86_64|"") ./tools/build.sh $(BUILD_ARGS) ;; \
	  riscv64|aarch64) ./tools/build.sh --arch $(ARCH) $(BUILD_ARGS) ;; \
	  i686) ./tools/build-i686.sh ;; \
	  *) echo "make: unknown ARCH=$(ARCH) — use x86_64, riscv64, aarch64 or i686" >&2; exit 2 ;; \
	esac

build-riscv:
	./tools/build.sh --arch riscv64 $(BUILD_ARGS)

build-aarch64:
	./tools/build.sh --arch aarch64 $(BUILD_ARGS)

build-i686:
	./tools/build-i686.sh

build-bios:
	./tools/build-bios.sh

build-all: build build-i686 build-bios iso

iso:
	./tools/build-iso.sh

# The user payload is what kernel/build.rs embeds; it must exist before any
# cargo check of the kernel (build.sh does this too, via sync_embed).
payload:
	@[ -f kernel/user_program.bin ] || { \
	  echo "== building kernel/user_program.bin (the kernel build embeds it)"; \
	  cargo build -p fantuan-user --target x86_64-unknown-none --release && \
	  cp target/x86_64-unknown-none/release/fantuan-user kernel/user_program.bin; }

# Fast feedback: checks every crate that can be checked with a stable target
# and reports warnings, without building the shells, the C layer or booting
# anything. kernel-i686 is out of reach here (its own nightly toolchain pin and
# custom target JSON: use tools/build-i686.sh, or `make build ARCH=i686`).
check: payload check-lines
	cargo check -p fantuan-kernel --target x86_64-unknown-none --locked
	cargo check -p kernel-core --target riscv64gc-unknown-none-elf --locked
	cargo check -p kernel-core --target aarch64-unknown-none --locked
	cargo check -p kernel-riscv --target riscv64gc-unknown-none-elf --locked
	cargo check -p kernel-aarch64 --target aarch64-unknown-none --locked
	cargo check -p fantuan-user --target x86_64-unknown-none --locked
	cargo check -p fantuan-boot --target x86_64-unknown-uefi --locked

check-i686:
	./tools/build-i686.sh

# The 300-line rule as a ratchet: new offenders fail, and a budgeted file that
# dropped back under the limit must leave tools/line-budget.txt.
check-lines:
	./tools/check-lines.sh

# --- run ------------------------------------------------------------------
run:
	@case "$(ARCH)" in \
	  x86_64|"") ./tools/run.sh $(RUN_ARGS) ;; \
	  riscv64) ./tools/run.sh --arch riscv64 --disk --two-fs $(RUN_ARGS) ;; \
	  aarch64) ./tools/run.sh --arch aarch64 $(RUN_ARGS) ;; \
	  i686) ./tools/run-bios.sh --arch i686 ;; \
	  *) echo "make: unknown ARCH=$(ARCH)" >&2; exit 2 ;; \
	esac

run-graphics:
	./tools/run.sh --graphics $(RUN_ARGS)

run-riscv:
	./tools/run.sh --arch riscv64 --disk --two-fs $(RUN_ARGS)

run-aarch64:
	./tools/run.sh --arch aarch64 $(RUN_ARGS)

run-bios:
	./tools/run-bios.sh $(RUN_ARGS)

run-i686:
	./tools/run-bios.sh --arch i686 $(RUN_ARGS)

# --- verify ---------------------------------------------------------------
smoke:
	@rc=0; for s in $(SMOKE_SET); do \
	  echo "== smoke-$$s"; ./tools/smoke-$$s.sh $(SMOKE_ARGS) || rc=1; \
	done; exit $$rc

# One suite by name: `make smoke-gpl`, `make smoke-kms`, …
smoke-%:
	./tools/smoke-$*.sh $(SMOKE_ARGS)

# The x86 acceptance suite (tools/smoke.sh): 13 phases, ~25 minutes.
test:
	./tools/smoke.sh

test-matrix:
	@rc=0; for s in $(MATRIX_SET); do \
	  case "$$s" in \
	    x86|smoke) script=./tools/smoke.sh ;; \
	    *) script=./tools/smoke-$$s.sh ;; \
	  esac; \
	  echo "== $$s ($$script)"; "$$script" $(SMOKE_ARGS) || rc=1; \
	done; exit $$rc

# --- apps and licensing ---------------------------------------------------
apps apps-list:
	python3 tools/appctl/appctl.py list

sbom:
	python3 tools/appctl/appctl.py sbom

# --- housekeeping ---------------------------------------------------------
# clean keeps build/ (smoke logs, disk images, the bash source cache);
# distclean removes it too. Both leave the tracked tree untouched.
clean:
	cargo clean
	rm -f kernel/user_program.bin kernel/hello_program.bin kernel/proc_test_program.bin \
	      kernel/ls_program.bin kernel/cat_program.bin kernel/dash_program.bin \
	      kernel/bash_program.bin kernel/bash_program.sha256 \
	      kernel-riscv/user_program.bin

distclean: clean
	rm -rf build target kernel-i686/target
