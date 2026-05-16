ROOT := ../..
CARGO ?= cargo
PROFILE ?= release
PROFILE_DIR := $(if $(filter release,$(PROFILE)),release,$(PROFILE))
OUT := $(ROOT)/.zero/bin/zero-rs
PROFILE_FLAG := $(if $(filter release,$(PROFILE)),--release,)

.PHONY: all check test clippy clean install-local determinism

all: $(OUT)

$(OUT): FORCE
	$(CARGO) build --workspace $(PROFILE_FLAG)
	@mkdir -p $(dir $(OUT))
	cp target/$(PROFILE_DIR)/zero $(OUT)

check:
	$(CARGO) check --workspace --all-targets

clippy:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

test:
	$(CARGO) test --workspace

determinism:
	$(CARGO) run -p xtask --quiet -- check-determinism

clean:
	$(CARGO) clean
	rm -f $(OUT)

install-local: $(OUT)
	@printf 'installed %s\n' "$(OUT)"

.PHONY: FORCE
FORCE:
