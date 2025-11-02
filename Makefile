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

	@unset CONDA_PREFIX && \
	source $(VENV_BIN)/activate && \
	maturin develop --profile release -E dev


.PHONY: install
install: ##  Install the crate as module in the current virtualenv
	maturin develop --uv -E dev


.PHONY: profiling
profiling:
	maturin develop --uv --profile profiling -E dev


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
	cargo clippy --fix --allow-dirty --allow-staged
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
	./pw uv run --group docs mkdocs serve

docs-build: ## Build the documentation site
	./pw uv run --group docs mkdocs build --strict


docs-check: ## Build the documentation site
	./pw uv run --group docs mkdocs build


.PHONY: help
help:  ## Display this help screen
	@echo -e "\033[1mAvailable commands:\033[0m"
	@grep -E '^[a-z.A-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}' | sort
