.DEFAULT_GOAL := help

PYTHONPATH=
SHELL=bash
VENV=.venv


# On Windows, `Scripts/` is used.
ifeq ($(OS),Windows_NT)
	VENV_BIN=$(VENV)/Scripts
else
	VENV_BIN=$(VENV)/bin
endif


.PHONY: deps
deps: ## Install dependencies
	./pw uv sync

.PHONY: profiling
profiling:
	maturin develop --uv --profile profiling


.PHONY: test
test: ## Run tests
	$(VENV_BIN)/pytest
	cargo test


.PHONY: lint fmt lint-rust lint-python fmt-rust fmt-python

lint: lint-rust lint-python  ## Run linting checks for Rust and Python code
fmt: fmt-rust fmt-python  ## Format Rust and Python code


fmt-python: ## Format Python code
	$(VENV_BIN)/ruff check . --fix
	$(VENV_BIN)/ruff format .


fmt-rust: ## Format Rust code
	cargo fmt --all


lint-python: ## Lint Python code
	$(VENV_BIN)/ruff check .
	$(VENV_BIN)/ruff format . --check


lint-rust: ## Lint Rust code
	cargo fmt --all --check
	cargo clippy


.PHONY: type-check
type-check: ## Run type checking
	$(VENV_BIN)/basedpyright


.PHONY: docs docs-serve docs-build
docs: docs-serve ## Alias for docs-serve

docs-serve: ## Serve documentation locally with live reloading
	./pw uv run --group docs zensical serve

docs-build: ## Build the documentation site
	./pw uv run --group docs zensical build --strict


docs-check: ## Build the documentation site
	./pw uv run --group docs zensical build


ensure-debugging-enabled: ## enable support for attaching debuggers (required for debugging the language server in the vscode extension)
	./pw uv run scripts/ensure_debugging_enabled.py


.PHONY: help
help:  ## Display this help screen
	@echo -e "\033[1mAvailable commands:\033[0m"
	@grep -E '^[a-z.A-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}' | sort
