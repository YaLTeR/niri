### Why is the border/focus ring showing up through semitransparent windows?

Uncomment the `prefer-no-csd` setting at the [top level](./Configuration:-Miscellaneous.md) of the config.
Niri will draw focus rings and borders *around* windows that agree to omit their client-side decorations.

By default, focus ring and border are rendered as a solid background rectangle behind windows.
That is, they will show up through semitransparent windows.
This is because windows using client-side decorations can have an arbitrary shape.

You can also override this behavior with the `draw-border-with-background` [window rule](https://github.com/YaLTeR/niri/wiki/Configuration:-Window-Rules).

### Why is the Waybar pop-up menu showing behind windows?

Set `"layer": "top"` in your Waybar config.

Niri currently draws pop-up menus on the same layer as their parent surface.
By default, Waybar is on the `bottom` layer, which is behind windows, so Waybar pop-up menus also show behind windows.
