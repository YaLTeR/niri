### Overview

In the `layout {}` section you can change various settings that influence how windows are positioned and sized.

Here are the contents of this section at a glance:

```
layout {
    gaps 16
    center-focused-column "never"

    preset-column-widths {
        proportion 0.33333
        proportion 0.5
        proportion 0.66667
    }

    default-column-width { proportion 0.5; }

    focus-ring {
        // off
        width 4
        active-color "#7fc8ff"
        inactive-color "#505050"
        // active-gradient from="#80c8ff" to="#bbddff" angle=45
        // inactive-gradient from="#505050" to="#808080" angle=45 relative-to="workspace-view"
    }

    border {
        off
        width 4
        active-color "#ffc87f"
        inactive-color "#505050"
        // active-gradient from="#ffbb66" to="#ffc880" angle=45 relative-to="workspace-view"
        // inactive-gradient from="#505050" to="#808080" angle=45 relative-to="workspace-view"
    }

    struts {
        // left 64
        // right 64
        // top 64
        // bottom 64
    }
}
```

### `gaps`

Set gaps around (inside and outside) windows in logical pixels.

```
layout {
    gaps 16
}
```

### `center-focused-column`

When to center a column when changing focus.
This can be set to:

- `"never"`: no special centering, focusing an off-screen column will scroll it to the left or right edge of the screen. This is the default.
- `"always"`, the focused column will always be centered.
- `"on-overflow"`, focusing a column will center it if it doesn't fit on screen together with the previously focused column.

```
layout {
    center-focused-column "always"
}
```

### `preset-column-widths`

Set the widths that the `switch-preset-column-width` action (Mod+R) toggles between.

`proportion` sets the width as a fraction of the output width, taking gaps into account.
For example, you can perfectly fit four windows sized `proportion 0.25` on an output, regardless of the gaps setting.
The default preset widths are <sup>1</sup>&frasl;<sub>3</sub>, <sup>1</sup>&frasl;<sub>2</sub> and <sup>2</sup>&frasl;<sub>3</sub> of the output.

`fixed` sets the width in logical pixels exactly.

```
layout {
    // Cycle between 1/3, 1/2, 2/3 of the output, and a fixed 1280 logical pixels.
    preset-column-widths {
        proportion 0.33333
        proportion 0.5
        proportion 0.66667
        fixed 1280
    }
}
```

> [!NOTE]
> Currently, due to an oversight, a preset `fixed` width does not take borders into account.
> I.e., preset `fixed 1000` with 4-wide borders will make the window 992 logical pixels wide.
> This may eventually be corrected.
>
> All other ways of using `fixed` (i.e. `default-column-width` or `set-column-width`) do take borders into account and give you the exact window width that you request.

### `default-column-width`

Set the default width of the new windows.

The syntax is the same as in `preset-column-widths` above.

```
layout {
    // Open new windows sized 1/3 of the output.
    default-column-width { proportion 0.33333; }
}
```

You can also leave the brackets empty, then the windows themselves will decide their initial width.

```
layout {
    // New windows decide their initial width themselves.
    default-column-width {}
}
```

> [!NOTE]
> `default-column-width {}` causes niri to send a (0, H) size in the initial configure request.
>
> This is a bit [unclearly defined](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/issues/155) in the Wayland protocol, so some clients may misinterpret it.
> In practice, the only problematic client I saw is [foot](https://codeberg.org/dnkl/foot/), which takes this as a request to have a literal zero width.
>
> Either way, `default-column-width {}` is most useful for specific windows, in form of a [window rule](https://github.com/YaLTeR/niri/wiki/Configuration:-Window-Rules) with the same syntax.

### `focus-ring` and `border`

Focus ring and border are drawn around windows and indicate the active window.
They are very similar and have the same options.

The difference is that the focus ring is drawn only around the active window, whereas borders are drawn around all windows and affect their sizes (windows shrink to make space for the borders).

| Focus Ring | Border |
| ---------- | ------ |
| ![](./img/focus-ring.png) | ![](./img/border.png) |

> [!TIP]
> By default, focus ring and border are rendered as a solid background rectangle behind windows.
> That is, they will show up through semitransparent windows.
> This is because windows using client-side decorations can have an arbitrary shape.
> 
> If you don't like that, you should uncomment the `prefer-no-csd` setting at the [top level](./Configuration:-Miscellaneous.md) of the config.
> Niri will draw focus rings and borders *around* windows that agree to omit their client-side decorations.
> 
> Alternatively, you can override this behavior with the `draw-border-with-background` [window rule](https://github.com/YaLTeR/niri/wiki/Configuration:-Window-Rules).

Focus ring and border have the following options.

```
layout {
    // focus-ring has the same options.
    border {
        // Uncomment this line to disable the border.
        // off

        // Width of the border in logical pixels.
        width 4

        active-color "#ffc87f"
        inactive-color "#505050"

        // active-gradient from="#ffbb66" to="#ffc880" angle=45 relative-to="workspace-view"
        // inactive-gradient from="#505050" to="#808080" angle=45 relative-to="workspace-view"
    }
}
```

#### Colors

Colors can be set in a variety of ways:

- CSS named colors: `"red"`
- RGB hex: `"#rgb"`, `"#rgba"`, `"#rrggbb"`, `"#rrggbbaa"`
- CSS-like notation: `"rgb(255, 127, 0)"`, `"rgba()"`, `"hsl()"` and a few others.

`active-color` is the color of the focus ring / border around the active window, and `inactive-color` is the color of the focus ring / border around all other windows.

The *focus ring* is only drawn around the active window on each monitor, so with a single monitor you will never see its `inactive-color`.
You will see it if you have multiple monitors, though.

There's also a *deprecated* syntax for setting colors with four numbers representing R, G, B and A: `active-color 127 200 255 255`.

#### Gradients

Similarly to colors, you can set `active-gradient` and `inactive-gradient`, which will take precedence.

Gradients are rendered the same as CSS [`linear-gradient(angle, from, to)`](https://developer.mozilla.org/en-US/docs/Web/CSS/gradient/linear-gradient).
The angle works the same as in `linear-gradient`, and is optional, defaulting to `180` (top-to-bottom gradient).
You can use any CSS linear-gradient tool on the web to set these up, like [this one](https://www.css-gradient.com/).

```
layout {
    focus-ring {
        active-gradient from="#80c8ff" to="#bbddff" angle=45
    }
}
```

Gradients can be colored relative to windows individually (the default), or to the whole view of the workspace.
To do that, set `relative-to="workspace-view"`.
Here's a visual example:

| Default  | `relative-to="workspace-view"` |
| --- | --- |
| ![](./img/gradients-default.png) | ![](./img/gradients-relative-to-workspace-view.png) |

```
layout {
    border {
        active-gradient from="#ffbb66" to="#ffc880" angle=45 relative-to="workspace-view"
        inactive-gradient from="#505050" to="#808080" angle=45 relative-to="workspace-view"
    }
}
```

### `struts`

Struts shrink the area occupied by windows, similarly to layer-shell panels.
You can think of them as a kind of outer gaps.
They are set in logical pixels.

Left and right struts will cause the next window to the side to always peek out slightly.
Top and bottom struts will simply add outer gaps in addition to the area occupied by layer-shell panels and regular gaps.

```
layout {
    struts {
        left 64
        right 64
        top 64
        bottom 64
    }
}
```

![](./img/struts.png)
