# Working with Documentation

This guide explains how to work with Tach's documentation system, which uses Zensical.

## Prerequisites

You need Python installed on your system and the documentation dependencies:

```bash
./pw uv sync --only-group docs
```

!!! note

    if you've already followed [the setup instructions](./setting-up-project.md#1-automated-setup) then this step isn't required.

## Local Development

To work on the documentation locally, run:

```bash
zensical serve
```

This will start a local server at http://127.0.0.1:8000/ with live reloading.

## Documentation Structure

The documentation is organized as follows:

- `docs/index.md` - Home page
- `docs/getting-started/` - Getting started guides
- `docs/usage/` - Usage documentation
- `docs/contributing/` - Contributing guides
- `docs/assets/` - Images and other assets

## Adding New Pages

To add a new page:

1. Create a new Markdown file (`.md`) in the appropriate directory
2. Add an entry to the `nav` section in `mkdocs.yml`

Example:

```yaml
nav:
  - Home: index.md
  - Getting Started:
    - Overview: getting-started/introduction.md
    - # Add your new page here
    - My New Page: getting-started/my-new-page.md
```

## Formatting

Zensical uses Markdown for formatting. Some useful features with Zensical include:

### Code Blocks

```python
def example_function():
    return "Hello, World!"
```

### Admonitions

!!! note
    This is a note admonition.

!!! warning
    This is a warning admonition.

!!! tip
    This is a tip admonition.

### Tabs

=== "Tab 1"
    Content for tab 1

=== "Tab 2"
    Content for tab 2

## Images

Place images in the `docs/assets/` directory and reference them like this:

```markdown
![Alt text](../assets/image-name.png)
```

## Deployment

The documentation is automatically deployed to GitHub Pages when changes are pushed to the main branch. The deployment is handled by the GitHub Actions workflow in `.github/workflows/docs.yml`. 