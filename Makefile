#!/usr/bin/make -f

PIP := pipx
UV := uv
SHELL := /bin/bash

.PHONY: help lint check-docker check-npx lint-fix add-lint-hook clean test

help: ## Shows this help message
	@echo "Available commands:"
	@awk '/^[a-zA-Z_-]+:.*## / { printf "  %-20s %s\n", $$1, substr($$0, index($$0, "##") + 3) }' $(MAKEFILE_LIST)

add-lint-hook: ## Adds a git pre-push hook to automatically run 'lint' before pushing
	@echo "#!/bin/bash" > .git/hooks/pre-push
	@echo "make lint" >> .git/hooks/pre-push
	@chmod +x .git/hooks/pre-push
	@echo "make test" >> .git/hooks/pre-push
	@echo "Pre-push hook added. The 'lint' command will now run before each push."
	@echo "Pre-push hook added. The 'test' command will now run before each push."	

check-docker: ## Checks if docker is installed
	@if ! command -v docker &> /dev/null; then \
		echo "Docker is not installed. Please install it."; \
		exit 1; \
	else \
		echo "Docker version:"; \
		docker --version; \
	fi

check-npx: ## Checks if npx is installed
	@if ! command -v npx &> /dev/null; then \
		echo "npx is not installed. Please install it."; \
		exit 1; \
	else \
		echo "npx version:"; \
		npx --version; \
	fi

clean: ## Clean cache of uv and delete virtual environment
	@$(UV) cache clean
	@rm -rf .venv

lint: ## Lints the code
	@docker build --pull -q -t megalinter-rust-custom .github/megalinter-rust/
	@docker run --rm -v /var/run/docker.sock:/var/run/docker.sock:rw -v $(CURDIR):/tmp/lint:rw megalinter-rust-custom

lint-fix: ## Lints the code and fixes issues
	@docker build --pull -q -t megalinter-rust-custom .github/megalinter-rust/
	@docker run --rm -v /var/run/docker.sock:/var/run/docker.sock:rw -v $(CURDIR):/tmp/lint:rw -e APPLY_FIXES=all megalinter-rust-custom
