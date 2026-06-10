.DEFAULT_GOAL := help

.PHONY: help run check test build build-headless install uninstall \
        docker-up docker-down docker-logs docs sandbox-image

WEB_DIR := $(CURDIR)/apps/web
CARGO := cargo

help: ## Show available commands
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
	  | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

run: ## Run the bundled release server using ./config.toml
	@test -f "$(WEB_DIR)/out/index.html" || \
		(echo "Web bundle missing; run 'make build' first." >&2; exit 1)
	HELPCORE_CONFIG="$(CURDIR)/config.toml" \
	HELPCORE_EMBED_WEB_DIR="$(WEB_DIR)/out" \
	$(CARGO) run --release -p helpcore-server

check: ## Type-check the Rust workspace
	$(CARGO) check --workspace

test: ## Run the Rust test suite
	$(CARGO) test --workspace

build: ## Build release binaries with the web UI embedded
	cd "$(WEB_DIR)" && npm ci
	cd "$(WEB_DIR)" && NEXT_EXPORT=true npm run build
	HELPCORE_EMBED_WEB_DIR="$(WEB_DIR)/out" $(CARGO) build --release \
		-p helpcore-server -p helpcore-cli

build-headless: ## Build release binaries without the web UI
	$(CARGO) build --release -p helpcore-server -p helpcore-cli

docs: ## Generate Rust API documentation (open target/doc/helpcore_api/index.html)
	$(CARGO) doc --workspace --no-deps
	@echo "Docs generated in target/doc/"

docs-check: ## Check that Rust API docs build without warnings (CI gate)
	RUSTDOCFLAGS="-D warnings" $(CARGO) doc --workspace --no-deps

install: ## Install and start helpcore as a macOS LaunchAgent
	./packaging/macos/install.sh

uninstall: ## Remove the macOS LaunchAgent and binaries
	./packaging/macos/uninstall.sh

docker-up: ## Start the Docker stack
	docker compose up -d

docker-down: ## Stop the Docker stack
	docker compose down

docker-logs: ## Follow server logs
	docker compose logs -f helpcore

sandbox-image: ## Build the sandbox container image (helpcore-sandbox:latest)
	docker build -t helpcore-sandbox:latest -f docker/sandbox.Dockerfile docker
