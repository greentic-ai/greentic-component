CARGO ?= cargo
SMOKE_COMPONENT ?= smoke-demo
SMOKE_ARGS ?=

ifndef SMOKE_DIR
SMOKE_BASE := $(shell mktemp -d 2>/dev/null || mktemp -d -t greentic-smoke)
SMOKE_DIR := $(SMOKE_BASE)/$(SMOKE_COMPONENT)
SMOKE_CLEANUP := 1
else
SMOKE_CLEANUP := 0
endif

.PHONY: build test lint smoke fmt fmt-check clippy

build:
	$(CARGO) build --all-targets

test:
	$(CARGO) test --all-features

fmt:
	$(CARGO) fmt

fmt-check:
	$(CARGO) fmt --all -- --check

clippy:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

lint: fmt-check clippy

smoke:
	rm -rf $(SMOKE_DIR)
	mkdir -p $(SMOKE_DIR)
	$(CARGO) run -p greentic-component --features cli --bin greentic-component -- \
		new --name $(SMOKE_COMPONENT) --org ai.greentic \
		--path $(SMOKE_DIR)/$(SMOKE_COMPONENT) --non-interactive --no-check $(SMOKE_ARGS)
	$(CARGO) run -p greentic-component --features cli --bin component-doctor -- \
		$(SMOKE_DIR)/$(SMOKE_COMPONENT)
	$(CARGO) run -p greentic-component --features cli --bin component-inspect -- \
		--json $(SMOKE_DIR)/$(SMOKE_COMPONENT)/component.manifest.json
	cd $(SMOKE_DIR)/$(SMOKE_COMPONENT) && $(CARGO) check --target wasm32-wasip2
	if [ "$(SMOKE_CLEANUP)" = "1" ] && [ -n "$(SMOKE_BASE)" ]; then rm -rf "$(SMOKE_BASE)"; fi
