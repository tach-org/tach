# DTach - VS Code Extension


### Setup
1. Install the extension and you should see invalid imports highlighted as errors on save.
2. Navigate to the settings for the extension in `Extensions > Tach` to set arguments if needed. 
3. Run `Tach: Restart Server` from the command palette (`CMD+SHFT+P`) to restart the server.

If you run into any issues, let us know by submitting a [Github Issue](https://github.com/detachhead/dtach/issues)!


### Known Issues
To diagnose an issue, use 'Output: Focus on Output View' in the VSCode Command Palette, and then select 'Tach' from the dropdown selector to see the logs.

- 'Project config root not found': this means you don't have a `tach.toml` in the ancestors of an open file. This should generally be in the root of your repo.