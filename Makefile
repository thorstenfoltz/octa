#!/usr/bin/make -f

UV := uv
SHELL := /bin/bash
# The image CI runs (.github/workflows/megalinter.yml); /tmp/lint is its default workspace.
MEGALINTER_IMAGE := ghcr.io/oxsecurity/megalinter-rust:v10
MEGALINTER := docker run --rm -v "$(CURDIR):/tmp/lint"

.PHONY: help lint check-docker lint-fix add-lint-hook clean

help: ## Shows this help message
	@echo "Available commands:"
	@awk '/^[a-zA-Z_-]+:.*## / { printf "  %-20s %s\n", $$1, substr($$0, index($$0, "##") + 3) }' $(MAKEFILE_LIST)

# The hook keeps Git LFS's own pre-push step, which the repo needs to upload
# its LFS files; </dev/null leaves git's ref list on stdin for it.
add-lint-hook: ## Adds a git pre-push hook to automatically run 'lint' before pushing
	@echo "#!/bin/sh" > .git/hooks/pre-push
	@echo 'make lint </dev/null && exec git lfs pre-push "$$@"' >> .git/hooks/pre-push
	@chmod +x .git/hooks/pre-push
	@echo "Pre-push hook added. The 'lint' command will now run before each push."

check-docker: ## Checks if docker is installed
	@if ! command -v docker &> /dev/null; then \
		echo "Docker is not installed. Please install it."; \
		exit 1; \
	else \
		echo "Docker version:"; \
		docker --version; \
	fi

clean: ## Clean cache of uv and delete virtual environment
	@$(UV) cache clean
	@rm -rf .venv

lint: check-docker ## Lints the code (MegaLinter Rust flavour, the image CI uses)
	@$(MEGALINTER) $(MEGALINTER_IMAGE)

lint-fix: check-docker ## Lints the code and applies fixes
	@$(MEGALINTER) -e APPLY_FIXES=all $(MEGALINTER_IMAGE)
