### VSCode

There seems to be a bug in VSCode's Wayland backend until 1.86.0 which causes the window to not show up when using server-side decorations. So, to run VSCode:

1. Make sure VSCode is 1.86.0 or above, or that `prefer-no-csd` is **not set** in the niri config
2. Run `code --ozone-platform-hint=auto --enable-features=WaylandWindowDecorations`

Also, if you're having issues with some VSCode hotkeys, try starting `Xwayland` and setting the `DISPLAY=:0` environment variable for VSCode. That is, still running VSCode with the Wayland backend, but with `DISPLAY` set to a running Xwayland instance. Apparently, VSCode currently unconditionally queries the X server for a keymap.

### Chromium

When creating new windows within Chromium (e.g. with <kbd>Ctrl</kbd><kbd>N</kbd>), there's a Chromium bug with sizing:

- With CSD (`prefer-no-csd` unset), the window will be a bit smaller than needed
- With SSD (`prefer-no-csd` set), the window buffer will be offset to the top-left

Both of these can be fixed by resizing the new Chromium window.

### WezTerm

There's [a bug](https://github.com/wez/wezterm/issues/4708) in WezTerm that it waits for a zero-sized Wayland configure event, so its window never shows up in niri. To work around it, put this window rule in the niri config (included in the default config):

```kdl
window-rule {
    match app-id=r#"^org\.wezfurlong\.wezterm$"#
    default-column-width {}
}
```

This empty default column width lets WezTerm pick its own initial width which makes it show up properly.
