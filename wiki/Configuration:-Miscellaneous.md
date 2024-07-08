### Overview

This page documents all top-level options that don't otherwise have dedicated pages.

Here are all of these options at a glance:

```kdl
spawn-at-startup "waybar"
spawn-at-startup "alacritty"

prefer-no-csd

screenshot-path "~/Pictures/Screenshots/Screenshot from %Y-%m-%d %H-%M-%S.png"

environment {
    QT_QPA_PLATFORM "wayland"
    DISPLAY null
}

cursor {
    xcursor-theme "breeze_cursors"
    xcursor-size 48
}

hotkey-overlay {
    skip-at-startup
}
```

### `spawn-at-startup`

Add lines like this to spawn processes at niri startup.

`spawn-at-startup` accepts a path to the program binary as the first argument, followed by arguments to the program.

This option works the same way as the `spawn` key binding action, so please read about all its subtleties on the [key bindings](./Configuration:-Key-Bindings.md) page.

```kdl
spawn-at-startup "waybar"
spawn-at-startup "alacritty"
```

Note that running niri as a systemd session supports xdg-desktop-autostart out of the box, which may be more convenient to use.
Thanks to this, apps that you configured to autostart in GNOME will also "just work" in niri, without any manual `spawn-at-startup` configuration.

### `prefer-no-csd`

This flag will make niri ask the applications to omit their client-side decorations.

If an application will specifically ask for CSD, the request will be honored.
Additionally, clients will be informed that they are tiled, removing some rounded corners.

With `prefer-no-csd` set, applications that negotiate server-side decorations through the xdg-decoration protocol will have focus ring and border drawn around them *without* a solid colored background.

> [!NOTE]
> Unlike most other options, changing `prefer-no-csd` will not affect already running applications.
> This mainly has to do with niri working around a [bug in SDL2](https://github.com/libsdl-org/SDL/issues/8173) that prevents SDL2 applications from starting.
>
> Restart applications after changing `prefer-no-csd` in the config to apply it.

```kdl
prefer-no-csd
```

### `screenshot-path`

Set the path where screenshots are saved.
A `~` at the front will be expanded to the home directory.

The path is formatted with `strftime(3)` to give you the screenshot date and time.

Niri will create the last folder of the path if it doesn't exist.

```kdl
screenshot-path "~/Pictures/Screenshots/Screenshot from %Y-%m-%d %H-%M-%S.png"
```

You can also set this option to `null` to disable saving screenshots to disk.

```kdl
screenshot-path null
```

### `environment`

Override environment variables for processes spawned by niri.

```kdl
environment {
    // Set a variable like this:
    // QT_QPA_PLATFORM "wayland"

    // Remove a variable by using null as the value:
    // DISPLAY null
}
```

### `cursor`

Change the theme and size of the cursor as well as set the `XCURSOR_THEME` and `XCURSOR_SIZE` environment variables.

```kdl
cursor {
    xcursor-theme "breeze_cursors"
    xcursor-size 48
}
```

### `hotkey-overlay`

Settings for the "Important Hotkeys" overlay.

Set the `skip-at-startup` flag if you don't want to see the hotkey help at niri startup.

```kdl
hotkey-overlay {
    skip-at-startup
}
```
