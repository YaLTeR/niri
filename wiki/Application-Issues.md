### VSCode

If you're having issues with some VSCode hotkeys, try starting `Xwayland` and setting the `DISPLAY=:0` environment variable for VSCode.
That is, still running VSCode with the Wayland backend, but with `DISPLAY` set to a running Xwayland instance.
Apparently, VSCode currently unconditionally queries the X server for a keymap.

### WezTerm

There's [a bug](https://github.com/wez/wezterm/issues/4708) in WezTerm that it waits for a zero-sized Wayland configure event, so its window never shows up in niri. To work around it, put this window rule in the niri config (included in the default config):

```kdl
window-rule {
    match app-id=r#"^org\.wezfurlong\.wezterm$"#
    default-column-width {}
}
```

This empty default column width lets WezTerm pick its own initial width which makes it show up properly.

### Ghidra

Some Java apps like Ghidra can show up blank under xwayland-satellite.
To fix this, run them with the `_JAVA_AWT_WM_NONREPARENTING=1` environment variable
