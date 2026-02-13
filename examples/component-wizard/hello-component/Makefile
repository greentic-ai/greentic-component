SHELL := /bin/sh

NAME := $(shell awk 'BEGIN{in=0} /^\[package\]/{in=1; next} /^\[/{in=0} in && /^name = / {gsub(/"/ , "", $$3); print $$3; exit}' Cargo.toml)
ABI_VERSION := $(shell awk 'BEGIN{in=0} /^\[package.metadata.greentic\]/{in=1; next} /^\[/{in=0} in && /^abi_version = / {gsub(/"/ , "", $$3); print $$3; exit}' Cargo.toml)
ABI_VERSION_UNDERSCORE := $(subst .,_,$(ABI_VERSION))
DIST_DIR := dist
WASM_OUT := $(DIST_DIR)/$(NAME)__$(ABI_VERSION_UNDERSCORE).wasm
WASM_SRC := target/wasm32-wasip2/release/$(NAME).wasm

.PHONY: build test fmt clippy wasm doctor

build:
	cargo build

test:
	cargo test

fmt:
	cargo fmt

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

wasm:
	if cargo component --version >/dev/null 2>&1; then \
		cargo component build --release; \
	else \
		cargo build --target wasm32-wasip2 --release; \
	fi
	mkdir -p $(DIST_DIR)
	cp $(WASM_SRC) $(WASM_OUT)

doctor:
	greentic-component doctor $(WASM_OUT)
