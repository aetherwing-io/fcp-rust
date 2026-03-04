.PHONY: test build release clippy help

test: ## Run all tests
	cargo test

build: ## Build debug binary
	cargo build

release: ## Build release binary and install to ~/.local/bin
	cargo build --release
	ln -sf $(CURDIR)/target/release/fcp-rust ~/.local/bin/fcp-rust

clippy: ## Run clippy lints
	cargo clippy -- -D warnings

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'
