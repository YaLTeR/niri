### Overview

<sup>Since: next release</sup>

Layer rules let you adjust behavior for individual layer-shell surfaces.
They have `match` and `exclude` directives that control which layer-shell surfaces the rule should apply to, and a number of properties that you can set.

Layer rules are processed and work very similarly to window rules, just with different matchers and properties.
Please read the [window rules](./Configuration:-Window-Rules.md) wiki page to learn how matching works.

Here are all matchers and properties that a layer rule could have:

```kdl
layer-rule {
    match namespace="waybar"
    match at-startup=true

    // Properties that apply continuously.
    opacity 0.5
    block-out-from "screencast"
    // block-out-from "screen-capture"
}
```

### Layer Surface Matching

Let's look at the matchers in more detail.

#### `namespace`

This is a regular expression that should match anywhere in the surface namespace.
You can read about the supported regular expression syntax [here](https://docs.rs/regex/latest/regex/#syntax).

```kdl
// Match surfaces with namespace containing "waybar",
layer-rule {
    match namespace="waybar"
}
```

You can find the namespaces of all open layer-shell surfaces by running `niri msg layers`.

#### `at-startup`

Can be `true` or `false`.
Matches during the first 60 seconds after starting niri.

```kdl
// Show layer-shell surfaces with 0.5 opacity at niri startup, but not afterwards.
layer-rule {
    match at-startup=true

    opacity 0.5
}
```

### Dynamic Properties

These properties apply continuously to open layer-shell surfaces.

#### `block-out-from`

You can block out surfaces from xdg-desktop-portal screencasts or all screen captures.
They will be replaced with solid black rectangles.

This can be useful for notifications.

The same caveats and instructions apply as for the `block-out-from` window rule.
Please read the `block-out-from` section in the [window rules](./Configuration:-Window-Rules.md) wiki page for more details.

![Screenshot showing a notification visible normally, but blocked out on OBS.](./img/layer-block-out-from-screencast.png)

```kdl
// Block out mako notifications from screencasts.
layer-rule {
    match namespace="^notifications$"

    block-out-from "screencast"
}
```

#### `opacity`

Set the opacity of the surface.
`0.0` is fully transparent, `1.0` is fully opaque.
This is applied on top of the surface's own opacity, so semitransparent surfaces will become even more transparent.

Opacity is applied to every child of the layer-shell surface individually, so subsurfaces and pop-up menus will show window content behind them.

```kdl
// Make fuzzel semitransparent.
layer-rule {
    match namespace="^launcher$"

    opacity 0.95
}
```
