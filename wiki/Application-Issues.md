### Electron based applications

Electron based applications can work directly on Wayland, but they don't accept it without configuration. Configuration differs between versions. Information was grabbed from the [ArchWiki](https://wiki.archlinux.org/title/Wayland#Electron).

For Electron (>28) you need to set environment variable:
```kdl
environment {
    ELECTRON_OZONE_PLATFORM_HINT "auto"
}
```

For previous version you need to set flags in configuration file `~/.config/electron-flags.conf`:
```
--enable-features=UseOzonePlatform
--ozone-platform-hint=auto
```

### VSCode

To run VSCode natively via Wayland you need to put flags in `~/.config/code-flags.conf`:
```
--enable-features=UseOzonePlatform
--ozone-platform-hint=auto
```

### WezTerm

> [!NOTE]
> Both of these issues seem to be fixed in the nightly build of WezTerm.

There's [a bug](https://github.com/wezterm/wezterm/issues/4708) in WezTerm that it waits for a zero-sized Wayland configure event, so its window never shows up in niri. To work around it, put this window rule in the niri config (included in the default config):

```kdl
window-rule {
    match app-id=r#"^org\.wezfurlong\.wezterm$"#
    default-column-width {}
}
```

This empty default column width lets WezTerm pick its own initial width which makes it show up properly.

There's [another bug](https://github.com/wezterm/wezterm/issues/6472) in WezTerm that causes it to choose a wrong size when it's in a tiled state, and prevent resizing it.
Niri puts windows in the tiled state with [`prefer-no-csd`](./Configuration:-Miscellaneous.md#prefer-no-csd).
So if you hit this problem, comment out `prefer-no-csd` in the niri config and restart WezTerm.

### Ghidra

Some Java apps like Ghidra can show up blank under xwayland-satellite.
To fix this, run them with the `_JAVA_AWT_WM_NONREPARENTING=1` environment variable.

### rofi-wayland

There's a bug in rofi-wayland that prevents it from accepting keyboard input on niri with errors in the output.
It's been fixed in rofi, but [the fix had not been released yet](https://github.com/davatorium/rofi/discussions/2008).

### Fullscreen games

Some video games, both Linux-native and on Wine, have various issues when using non-stacking desktop environments.
Most of these can be avoided with Valve's [gamescope](https://github.com/ValveSoftware/gamescope), for example:

```sh
gamescope -f -w 1920 -h 1080 -W 1920 -H 1080 --force-grab-cursor --backend sdl -- <game>
```

This command will run *<game>* in 1080p fullscreen—make sure to replace the width and height values to match your desired resolution.
`--force-grab-cursor` forces gamescope to use relative mouse movement which prevents the cursor from escaping the game's window on multi-monitor setups.
Note that `--backend sdl` is currently also required as gamescope's default Wayland backend doesn't lock the cursor properly (possibly related to https://github.com/ValveSoftware/gamescope/issues/1711).

Steam users should use gamescope through a game's [launch options](https://help.steampowered.com/en/faqs/view/7D01-D2DD-D75E-2955) by replacing the game executable with `%command%`.
Other game launchers such as [Lutris](https://lutris.net/) have their own ways of setting gamescope options.

Running X11-based games with this method doesn't require Xwayland as gamescope creates its own Xwayland server.
You can run Wayland-native games as well by passing `--expose-wayland` to gamescope, therefore eliminating X11 from the equation.
