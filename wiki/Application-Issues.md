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

There's [another bug](https://github.com/wez/wezterm/issues/6472) in WezTerm that causes it to choose a wrong size when it's in a tiled state, and prevent resizing it.
Niri puts windows in the tiled state with `prefer-no-csd`.
So if you hit this problem, comment out `prefer-no-csd` in the niri config and restart WezTerm.

### Ghidra

Some Java apps like Ghidra can show up blank under xwayland-satellite.
To fix this, run them with the `_JAVA_AWT_WM_NONREPARENTING=1` environment variable.

### rofi-wayland

There's a bug in rofi-wayland that prevents it from accepting keyboard input on niri with errors in the output.
It's been fixed in rofi, but the fix had not been released yet: https://github.com/davatorium/rofi/discussions/2008
