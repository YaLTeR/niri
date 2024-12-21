### Overview

Niri has several options that are only useful for debugging, or are experimental and have known issues.
They are not meant for normal use.

> [!CAUTION]
> These options are **not** covered by the [config breaking change policy](./Configuration:-Overview.md).
> They can change or stop working at any point with little notice.

Here are all the options at a glance:

```kdl
debug {
    preview-render "screencast"
    // preview-render "screen-capture"
    enable-overlay-planes
    disable-cursor-plane
    disable-direct-scanout
    render-drm-device "/dev/dri/renderD129"
    force-pipewire-invalid-modifier
    dbus-interfaces-in-non-session-instances
    wait-for-frame-completion-before-queueing
    emulate-zero-presentation-time
    disable-resize-throttling
    disable-transactions
    keep-laptop-panel-on-when-lid-is-closed
    disable-monitor-names
    strict-new-window-focus-policy
}

binds {
    Mod+Shift+Ctrl+T { toggle-debug-tint; }
    Mod+Shift+Ctrl+O { debug-toggle-opaque-regions; }
    Mod+Shift+Ctrl+D { debug-toggle-damage; }
}
```

### `preview-render`

Make niri render the monitors the same way as for a screencast or a screen capture.

Useful for previewing the `block-out-from` window rule.

```kdl
debug {
    preview-render "screencast"
    // preview-render "screen-capture"
}
```

### `enable-overlay-planes`

Enable direct scanout into overlay planes.
May cause frame drops during some animations on some hardware (which is why it is not the default).

Direct scanout into the primary plane is always enabled.

```kdl
debug {
    enable-overlay-planes
}
```

### `disable-cursor-plane`

Disable the use of the cursor plane.
The cursor will be rendered together with the rest of the frame.

Useful to work around driver bugs on specific hardware.

```kdl
debug {
    disable-cursor-plane
}
```

### `disable-direct-scanout`

Disable direct scanout to both the primary plane and the overlay planes.

```kdl
debug {
    disable-direct-scanout
}
```

### `render-drm-device`

Override the DRM device that niri will use for all rendering.

You can set this to make niri use a different primary GPU than the default one.

```kdl
debug {
    render-drm-device "/dev/dri/renderD129"
}
```

### `force-pipewire-invalid-modifier`

<sup>Since: next release</sup>

Forces PipeWire screencasting to use the invalid modifier, even when DRM offers more modifiers.

Useful for testing the invalid modifier code path that is hit by drivers that don't support modifiers.

```kdl
debug {
    force-pipewire-invalid-modifier
}
```

### `dbus-interfaces-in-non-session-instances`

Make niri create its D-Bus interfaces even if it's not running as a `--session`.

Useful for testing screencasting changes without having to relogin.

The main niri instance will *not* currently take back the interfaces when you close the test instance, so you will need to relogin in the end to make screencasting work again.

```kdl
debug {
    dbus-interfaces-in-non-session-instances
}
```

### `wait-for-frame-completion-before-queueing`

Wait until every frame is done rendering before handing it over to DRM.

Useful for diagnosing certain synchronization and performance problems.

```kdl
debug {
    wait-for-frame-completion-before-queueing
}
```

### `emulate-zero-presentation-time`

Emulate zero (unknown) presentation time returned from DRM.

This is a thing on NVIDIA proprietary drivers, so this flag can be used to test that niri doesn't break too hard on those systems.

```kdl
debug {
    emulate-zero-presentation-time
}
```

### `disable-resize-throttling`

<sup>Since: 0.1.9</sup>

Disable throttling resize events sent to windows.

By default, when resizing quickly (e.g. interactively), a window will only receive the next size once it has made a commit for the previously requested size.
This is required for resize transactions to work properly, and it also helps certain clients which don't batch incoming resizes from the compositor.

Disabling resize throttling will send resizes to windows as fast as possible, which is potentially very fast (for example, on a 1000 Hz mouse).

```kdl
debug {
    disable-resize-throttling
}
```

### `disable-transactions`

<sup>Since: 0.1.9</sup>

Disable transactions (resize and close).

By default, windows which must resize together, do resize together.
For example, all windows in a column must resize at the same time to maintain the combined column height equal to the screen height, and to maintain the same window width.

Transactions make niri wait until all windows finish resizing before showing them all on screen in one, synchronized frame.
For them to work properly, resize throttling shouldn't be disabled (with the previous debug flag).

```kdl
debug {
    disable-transactions
}
```

### `keep-laptop-panel-on-when-lid-is-closed`

<sup>Since: 0.1.10</sup>

By default, niri will disable the internal laptop monitor when the laptop lid is closed.
This flag turns off this behavior and will leave the internal laptop monitor on.

```kdl
debug {
    keep-laptop-panel-on-when-lid-is-closed
}
```

### `disable-monitor-names`

<sup>Since: 0.1.10</sup>

Disables the make/model/serial monitor names, as if niri fails to read them from the EDID.

Use this flag to work around a crash present in 0.1.9 and 0.1.10 when connecting two monitors with matching make/model/serial.

```kdl
debug {
    disable-monitor-names
}
```

### `strict-new-window-focus-policy`

<sup>Since: next release</sup>

Disables heuristic automatic focusing for new windows.
Only windows that activate themselves with a valid xdg-activation token will be focused.

```kdl
debug {
    strict-new-window-focus-policy
}
```

### Key Bindings

These are not debug options, but rather key bindings.

#### `toggle-debug-tint`

Tints all surfaces green, unless they are being directly scanned out.

Useful to check if direct scanout is working.

```kdl
binds {
    Mod+Shift+Ctrl+T { toggle-debug-tint; }
}
```

#### `debug-toggle-opaque-regions`

<sup>Since: 0.1.6</sup>

Tints regions marked as opaque with blue and the rest of the render elements with red.

Useful to check how Wayland surfaces and internal render elements mark their parts as opaque, which is a rendering performance optimization.

```kdl
binds {
    Mod+Shift+Ctrl+O { debug-toggle-opaque-regions; }
}
```

#### `debug-toggle-damage`

<sup>Since: 0.1.6</sup>

Tints damaged regions with red.

```kdl
binds {
    Mod+Shift+Ctrl+D { debug-toggle-damage; }
}
```
