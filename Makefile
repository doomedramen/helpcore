.PHONY: help dev check test build docker-build docker-smoke-amd64 docker-up docker-up-ollama \
        docker-down docker-logs docker-shell ollama-pull config \
        prod-pull prod-up prod-up-ollama prod-down prod-logs prod-update

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
	  | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

# ── Development ──────────────────────────────────────────────────────────────

dev: ## Run the server (requires config.toml)
	cargo run -p helpcore-server

check: ## Type-check all packages without building
	cargo check --workspace

test: ## Run all tests
	cargo test --workspace

build: ## Build all packages (debug)
	cargo build --workspace

build-release: ## Build release binaries for all packages
	cargo build --release --workspace

# ── Configuration ─────────────────────────────────────────────────────────────

config: ## Create config.toml from config.toml.example (if not already present)
	@if [ -f config.toml ]; then \
		echo "config.toml already exists — edit it directly"; \
	elif [ -d config.toml ]; then \
		if rmdir config.toml 2>/dev/null; then \
			cp config.toml.example config.toml; \
			echo "Replaced empty config.toml directory with a config file — edit it before starting the server"; \
		else \
			echo "Error: config.toml is a non-empty directory; move or remove it, then run 'make config' again"; \
			exit 1; \
		fi; \
	else \
		cp config.toml.example config.toml; \
		echo "Created config.toml — edit it before starting the server"; \
	fi

# ── Docker ────────────────────────────────────────────────────────────────────

docker-build: ## Build the Docker image
	docker compose build

docker-smoke-amd64: ## Build amd64 locally and run the Dockerfile startup check
	docker buildx build --platform linux/amd64 --load -t helpcore:amd64-local .

docker-up: config ## Start helpcore (creates config.toml from example if missing)
	docker compose up -d

docker-up-ollama: config ## Start helpcore + Ollama sidecar
	docker compose --profile with-ollama up -d

docker-down: ## Stop and remove containers
	docker compose down

docker-logs: ## Tail helpcore logs
	docker compose logs -f helpcore

docker-shell: ## Open a shell in the running helpcore container
	docker compose exec helpcore sh

# ── Ollama helpers ────────────────────────────────────────────────────────────

ollama-pull: ## Pull the default llama3 model into the Ollama sidecar
	docker compose exec ollama ollama pull llama3

ollama-list: ## List models in the Ollama sidecar
	docker compose exec ollama ollama list

# ── Production (home server / Proxmox) ───────────────────────────────────────
# Uses docker-compose.prod.yml — pulls pre-built image from GHCR.
PROD = docker compose -f docker-compose.prod.yml

prod-pull: ## Pull latest image from GHCR
	$(PROD) pull

prod-up: ## Start production stack using its inline config
	$(PROD) up -d

prod-up-ollama: ## Start production stack + Ollama sidecar
	$(PROD) --profile with-ollama up -d

prod-down: ## Stop production stack
	$(PROD) down

prod-logs: ## Tail production helpcore logs
	$(PROD) logs -f helpcore

prod-update: ## Pull latest image and restart (zero-downtime rolling update)
	$(PROD) pull helpcore
	$(PROD) up -d --no-deps helpcore
