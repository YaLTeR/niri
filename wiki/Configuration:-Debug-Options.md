### Overview

Niri has several options that are only useful for debugging, or are experimental and have known issues.
They are not meant for normal use.

> [!CAUTION]
> These options are **not** covered by the [config breaking change policy](./Configuration:-Overview.md#breaking-change-policy).
> They can change or stop working at any point with little notice.

Here are all the options at a glance:

```
debug {
    preview-render "screencast"
    // preview-render "screen-capture"
    enable-overlay-planes
    disable-cursor-plane
    render-drm-device "/dev/dri/renderD129"
    dbus-interfaces-in-non-session-instances
    wait-for-frame-completion-before-queueing
    emulate-zero-presentation-time
    enable-color-transformations-capability
}
```

### `preview-render`

Make niri render the monitors the same way as for a screencast or a screen capture.

Useful for previewing the [`block-out-from`](./Configuration:-Window-Rules.md#block-out-from) window rule.

```
debug {
    preview-render "screencast"
    // preview-render "screen-capture"
}
```

### `enable-overlay-planes`

Enable direct scanout into overlay planes.
May cause frame drops during some animations on some hardware (which is why it is not the default).

Direct scanout into the primary plane is always enabled.

```
debug {
    enable-overlay-planes
}
```

### `disable-cursor-plane`

Disable the use of the cursor plane.
The cursor will be rendered together with the rest of the frame.

Useful to work around driver bugs on specific hardware.

```
debug {
    disable-cursor-plane
}
```

### `render-drm-device`

Override the DRM device that niri will use for all rendering.

You can set this to make niri use a different primary GPU than the default one.

```
debug {
    render-drm-device "/dev/dri/renderD129"
}
```

### `dbus-interfaces-in-non-session-instances`

Make niri create its D-Bus interfaces even if it's not running as a `--session`.

Useful for testing screencasting changes without having to relogin.

The main niri instance will *not* currently take back the interfaces when you close the test instance, so you will need to relogin in the end to make screencasting work again.

```
debug {
    dbus-interfaces-in-non-session-instances
}
```

### `wait-for-frame-completion-before-queueing`

Wait until every frame is done rendering before handing it over to DRM.

Useful for diagnosing certain synchronization and performance problems.

```
debug {
    wait-for-frame-completion-before-queueing
}
```

### `emulate-zero-presentation-time`

Emulate zero (unknown) presentation time returned from DRM.

This is a thing on NVIDIA proprietary drivers, so this flag can be used to test that niri doesn't break too hard on those systems.

```
debug {
    emulate-zero-presentation-time
}
```

### `enable-color-transformations-capability`

Enable the color-transformations capability of the Smithay renderer.
May cause a slight decrease in rendering performance.

Currently, should cause no visible changes in behavior, but it will be needed for HDR support whenever that happens.
So, this flag exists to be able to make sure that nothing breaks.

```
debug {
    enable-color-transformations-capability
}
```
