### How to disable client-side decorations/make windows rectangular?

Uncomment the `prefer-no-csd` setting at the [top level](./Configuration:-Miscellaneous.md) of the config.
Then niri will ask windows to omit client-side decorations, and also inform them that they are being tiled (which makes some windows rectangular, even if they cannot omit the decorations).

Note that currently this will prevent edge window resize handles from showing up.
You can still resize windows by holding <kbd>Mod</kbd> and the right mouse button.

### Why is the border/focus ring showing up through semitransparent windows?

Uncomment the `prefer-no-csd` setting at the [top level](./Configuration:-Miscellaneous.md) of the config.
Niri will draw focus rings and borders *around* windows that agree to omit their client-side decorations.

By default, focus ring and border are rendered as a solid background rectangle behind windows.
That is, they will show up through semitransparent windows.
This is because windows using client-side decorations can have an arbitrary shape.

You can also override this behavior with the `draw-border-with-background` [window rule](./Configuration:-Window-Rules.md).

### Why is the Waybar pop-up menu showing behind windows?

Set `"layer": "top"` in your Waybar config.

Niri currently draws pop-up menus on the same layer as their parent surface.
By default, Waybar is on the `bottom` layer, which is behind windows, so Waybar pop-up menus also show behind windows.

### How to enable rounded corners for all windows?

Put this window rule in your config:

```kdl
window-rule {
    geometry-corner-radius 12
    clip-to-geometry true
}
```

For more information, check [this wiki section](https://github.com/YaLTeR/niri/wiki/Configuration:-Window-Rules#geometry-corner-radius).

### How to hide the "Important Hotkeys" pop-up at the start?

Put this into your config:

```kdl
hotkey-overlay {
    skip-at-startup
}
```

### How to run X11 apps like Steam or Discord?

To run X11 apps, you can use [xwayland-satellite](https://github.com/Supreeeme/xwayland-satellite).
Check [the Xwayland wiki page](./Xwayland.md) for instructions.

Keep in mind that you can run many Electron apps such as VSCode natively on Wayland by passing the right flags, e.g. `code --ozone-platform-hint=auto`
