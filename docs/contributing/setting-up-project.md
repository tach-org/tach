# Setup Guide

## 1. Automated dependency installation
Installing all the dependencies
```bash
 make deps
```

### Troubleshooting

#### Issue: `python: command not found`

On some systems, the `make deps` command may fail with an error stating that the `python` command is not found. This happens because some systems only provide `python3` and do not include a `python` symlink.

#### macOS Fix:
If you're on macOS, create a symbolic link for `python` pointing to `python3`:

```bash
sudo ln -s $(which python3) /usr/local/bin/python
```

#### Linux Fix:

```bash
sudo ln -s $(which python3) /usr/bin/python
```

#### Windows Fix:
On Windows, ensure Python is installed and added to your `PATH`. You can check by running:

```powershell
python --version
```

If `python` is not recognized, use the `python3` command instead, or create an alias in PowerShell:

```powershell
Set-Alias -Name python -Value python3
```

After applying the appropriate fix, retry:

```bash
make deps
```

## 2. Build

To build and rebuild after changes to Rust files.

!!! note
    Make sure you have the Rust compiler installed. This package requires Rust and Cargo to compile extensions.

### Install the crate as module in the current virtualenv

```bash
make install
```

## 3. Test

Tach internally uses `pytest` module for testing all the files within `python/tests/`
```bash
make test
```

## 4. Setting up the docs
Tach uses MkDocs with the Material theme for documentation. To work with the documentation:

1. Install the documentation dependencies:
```bash
pip install -r docs/requirements.txt
```

2. Start the local development server:
```bash
mkdocs serve
```

3. Open your browser to http://127.0.0.1:8000/ to see the documentation.

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
- [Good First Issues (For beginners)](https://github.com/detachhead/dtach/issues?q=is%3Aopen+is%3Aissue+label%3A%22good+first+issue%22)
- [Documentation Issues](https://github.com/detachhead/dtach/issues?q=is%3Aopen+is%3Aissue+label%3Adocumentation)
- [Issues](https://github.com/detachhead/dtach/issues)
- [Documentation](https://github.com/detachhead/dtach/tree/main/docs)
