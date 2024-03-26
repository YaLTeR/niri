### Overview

Window rules let you adjust behavior for individual windows.
They have `match` and `exclude` directives that control which windows the rule should apply to, and a number of properties that you can set.

Window rules are processed in order of appearance in the config file.
This means that you can put more generic rules first, then override them for specific windows later.
For example:

```
// Set open-maximized to true for all windows.
window-rule {
    open-maximized true
}

// Then, for Alacritty, set open-maximized back to false.
window-rule {
    match app-id="Alacritty"
    open-maximized false
}
```

Here are all matchers and properties that a window rule could have:

```
window-rule {
    match title="Firefox"
    match app-id="Alacritty"
    match is-active=true
    match is-focused=false

    // Properties that apply once upon window opening.
    default-column-width { proportion 0.75; }
    open-on-output "eDP-1"
    open-maximized true
    open-fullscreen true

    // Properties that apply continuously.
    draw-border-with-background false
    opacity 0.5
    block-out-from "screencast"
    // block-out-from "screen-capture"

    min-width 100
    max-width 200
    min-height 300
    max-height 300
}
```

### Window Matching

Each window rule can have several `match` and `exclude` directives.
In order for the rule to apply, a window needs to match *any* of the `match` directives, and *none* of the `exclude` directives.

```
window-rule {
    // Match all Telegram windows...
    match app-id=r#"^org\.telegram\.desktop$"#

    // ...except the media viewer window.
    exclude title="^Media viewer$"

    // Properties to apply.
    open-on-output "HDMI-A-1"
}
```

Match and exclude directives have the same syntax.
Here is what you can match on.

#### `title` and `app-id`

These are regular expressions that should match anywhere in the window title and app ID respectively.

```
// Match windows with title containing "Mozilla Firefox",
// or windows with app ID containing "Alacritty".
window-rule {
    match title="Mozilla Firefox"
    match app-id="Alacritty"
}
```

Raw KDL strings can be helpful for writing out regular expressions:

```
window-rule {
    exclude app-id=r#"^org\.keepassxc\.KeePassXC$"#
}
```

#### `is-active`

Can be `true` or `false`.
Matches active windows (same windows that have the active border / focus ring color).

Every workspace on the focused monitor will have one active window.
This means that you will usually have multiple active windows (one per workspace), and when you switch between workspaces, you can see two active windows at once.

```
window-rule {
    match is-active=true
}
```

#### `is-focused`

Can be `true` or `false`.
Matches the window that has the keyboard focus.

Contrary to `is-active`, there can only be a single focused window.
Also, when opening a layer-shell application launcher or pop-up menu, the keyboard focus goes to layer-shell.
While layer-shell has the keyboard focus, windows will not match this rule.

### Window Opening Properties

These properties apply once, when a window first opens.

To be precise, they apply at the point when niri sends the initial configure request to the window.

#### `default-column-width`

Set the default width for the new window.

```
window-rule {
    default-column-width { proportion 0.75; }
}
```

#### `open-on-output`

Make the window open on a specific output.

If such an output does not exist, the window will open on the currently focused output as usual.

If the window opens on an output that is not currently focused, the window will not be automatically focused.

```
window-rule {
    open-on-output "eDP-1"
}
```

#### `open-maximized`

Make the window open as a maximized column.

```
window-rule {
    open-maximized true
}
```

#### `open-fullscreen`

Make the window open fullscreen.

```
window-rule {
    open-fullscreen true
}
```

You can also set this to `false` to *prevent* a window from opening fullscreen.

```
window-rule {
    open-fullscreen false
}
```

### Dynamic Properties

These properties apply continuously to open windows.

#### `block-out-from`

You can block out windows from xdg-desktop-portal screencasts.
They will be replaced with solid black rectangles.

This can be useful for password managers or messenger windows, etc.

![Screenshot showing a window visible normally, but blocked out on OBS.](./img/block-out-from-screencast.png)

To preview and set up this rule, check the `preview-render` option in the debug section of the config.

> [!CAUTION]
> The window is **not** blocked out from third-party screenshot tools.
> If you open some screenshot tool with preview while screencasting, blocked out windows **will be visible** on the screencast.

The built-in screenshot UI is not affected though, you can use it safely, and windows will remain blocked out even when screencasting it.

```
window-rule {
    block-out-from "screencast"
}
```

Alternatively, you can block out the window out of *all* screen captures, including third-party screenshot tools.
This way you avoid accidentally showing the window on a screencast when opening a third-party screenshot preview.

This setting will still let you use the interactive built-in screenshot UI, but it will block out the window from the fully automatic screenshot actions, such as `screenshot-screen` and `screenshot-window`.
The reasoning is that with an interactive selection, you can make sure that you avoid screenshotting sensitive content.

```
window-rule {
    block-out-from "screen-capture"
}
```

#### `opacity`

Set the opacity of the window.
`0.0` is fully transparent, `1.0` is fully opaque.
This is applied on top of the window's own opacity, so semitransparent windows will become even more transparent.

Opacity is applied to every surface of the window individually, so subsurfaces and pop-up menus will show window content behind them.

![Screenshot showing Adwaita Demo with a semitransparent pop-up menu.](./img/opacity-popup.png)

Also, focus ring and border with background will show through semitransparent windows (see `prefer-no-csd` and the `draw-border-with-background` window rule below).

```
window-rule {
    opacity 0.9
}
```

#### `draw-border-with-background`

Override whether the border and the focus ring draw with a background.

Set this to `true` to draw them as solid colored rectangles even for windows which agreed to omit their client-side decorations.
Set this to `false` to draw them as borders around the window even for windows which use client-side decorations.

This property can be useful for rectangular windows that do not support the xdg-decoration protocol.

| With Background | Without Background |
| --------------- | ------------------ |
| ![](./img/simple-egl-border-with-background.png) | ![](./img/simple-egl-border-without-background.png) |

```
window-rule {
    draw-border-with-background false
}
```

#### Size Overrides

You can amend the window's minimum and maximum size in logical pixels.

Keep in mind that the window itself always has a final say in its size.
These values instruct niri to never ask the window to be smaller than the minimum you set, or to be bigger than the maximum you set.

> [!NOTE]
> `max-height` will only apply to automatically-sized windows if it is equal to `min-height`.
> Either set it equal to `min-height`, or change the window height manually after opening it with `set-window-height`.
>
> This is a limitation of niri's window height distribution algorithm.

```
window-rule {
    min-width 100
    max-width 200
    min-height 300
    max-height 300
}
