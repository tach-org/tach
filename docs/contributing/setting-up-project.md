# Setup Guide

Tach is primarily written in rust, but since it's purpose is to be used by python developers, we've made the setup process as simple as possible by making the rust compiler and other similar tools pinned as pypi dev dependencies using [uv](https://docs.astral.sh/uv/). This project also uses [pyprojectx](https://pyprojectx.github.io/) to manage its uv installation, so you should be able to get set up for local development as long as you have a python interpreter installed already.

## 1. Automated setup
Installing all the dependencies & build the project
```bash
./pw uv sync
```

## 2. Test

Tach internally uses `pytest` module for testing all the files within `python/tests/`
```bash
make test
```

## 3. Setting up the docs
Tach uses Zensical for documentation. To work with the documentation:

1. Start the local development server:
```bash
zensical serve
```

2. Open your browser to http://127.0.0.1:8000/ to see the documentation.

For more details, see [Working with Docs](working-with-docs.md).

## 5. Things to check before committing
Check and sync your dependencies in the root folder
```bash
tach check
tach sync
```
Type checking
```bash 
make type-check
```
Run linting checks for Rust and Python code
```bash
make lint
```
Format Rust and Python code
```bash
make fmt
```

That's it! You are now ready to push your new dev branch to your forked repo and then raise a PR with appropriate description

Find Beginner Friendly issues here: 
- [Good First Issues (For beginners)](https://github.com/gauge-sh/tach/issues?q=is%3Aopen+is%3Aissue+label%3A%22good+first+issue%22)
- [Documentation Issues](https://github.com/gauge-sh/tach/issues?q=is%3Aopen+is%3Aissue+label%3Adocumentation)
- [Issues](https://github.com/gauge-sh/tach/issues)
- [Documentation](https://github.com/gauge-sh/tach/tree/main/docs)
