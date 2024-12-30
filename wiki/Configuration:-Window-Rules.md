### Overview

Window rules let you adjust behavior for individual windows.
They have `match` and `exclude` directives that control which windows the rule should apply to, and a number of properties that you can set.

Window rules are processed in order of appearance in the config file.
This means that you can put more generic rules first, then override them for specific windows later.
For example:

```kdl
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

> [!TIP]
> In general, you cannot "unset" a property in a later rule, only set it to a different value.
> Use the `exclude` directives to avoid applying a rule for specific windows.

Here are all matchers and properties that a window rule could have:

```kdl
window-rule {
    match title="Firefox"
    match app-id="Alacritty"
    match is-active=true
    match is-focused=false
    match is-active-in-column=true
    match is-floating=true
    match at-startup=true

    // Properties that apply once upon window opening.
    default-column-width { proportion 0.75; }
    default-window-height { fixed 500; }
    open-on-output "Some Company CoolMonitor 1234"
    open-on-workspace "chat"
    open-maximized true
    open-fullscreen true
    open-floating true
    open-focused false

    // Properties that apply continuously.
    draw-border-with-background false
    opacity 0.5
    block-out-from "screencast"
    // block-out-from "screen-capture"
    variable-refresh-rate true
    default-floating-position x=100 y=200 relative-to="bottom-left"

    focus-ring {
        // off
        on
        width 4
        active-color "#7fc8ff"
        inactive-color "#505050"
        // active-gradient from="#80c8ff" to="#bbddff" angle=45
        // inactive-gradient from="#505050" to="#808080" angle=45 relative-to="workspace-view"
    }

    border {
        // Same as focus-ring.
    }

    geometry-corner-radius 12
    clip-to-geometry true

    min-width 100
    max-width 200
    min-height 300
    max-height 300
}
```

### Window Matching

Each window rule can have several `match` and `exclude` directives.
In order for the rule to apply, a window needs to match *any* of the `match` directives, and *none* of the `exclude` directives.

```kdl
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
There can be multiple *matchers* in one directive, then the window should match all of them for the directive to apply.

```kdl
window-rule {
    // Match Firefox windows with Gmail in title.
    match app-id="firefox" title="Gmail"
}

window-rule {
    // Match Firefox, but only when it is active...
    match app-id="firefox" is-active=true

    // ...or match Telegram...
    match app-id=r#"^org\.telegram\.desktop$"#

    // ...but don't match the Telegram media viewer.
    // If you open a tab in Firefox titled "Media viewer",
    // it will not be excluded because it doesn't match the app-id
    // of this exclude directive.
    exclude app-id=r#"^org\.telegram\.desktop$"# title="Media viewer"
}
```

Let's look at the matchers in more detail.

#### `title` and `app-id`

These are regular expressions that should match anywhere in the window title and app ID respectively.
You can read about the supported regular expression syntax [here](https://docs.rs/regex/latest/regex/#syntax).

```kdl
// Match windows with title containing "Mozilla Firefox",
// or windows with app ID containing "Alacritty".
window-rule {
    match title="Mozilla Firefox"
    match app-id="Alacritty"
}
```

Raw KDL strings can be helpful for writing out regular expressions:

```kdl
window-rule {
    exclude app-id=r#"^org\.keepassxc\.KeePassXC$"#
}
```

You can find the title and the app ID of the currently focused window by running `niri msg focused-window`.

> [!TIP]
> Another way to find the window title and app ID is to configure the `wlr/taskbar` module in [Waybar](https://github.com/Alexays/Waybar) to include them in the tooltip:
> 
> ```json
> "wlr/taskbar": {
>     "tooltip-format": "{title} | {app_id}",
> }
> ```

#### `is-active`

Can be `true` or `false`.
Matches active windows (same windows that have the active border / focus ring color).

Every workspace on the focused monitor will have one active window.
This means that you will usually have multiple active windows (one per workspace), and when you switch between workspaces, you can see two active windows at once.

```kdl
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

```kdl
window-rule {
    match is-focused=true
}
```

#### `is-active-in-column`

<sup>Since: 0.1.6</sup>

Can be `true` or `false`.
Matches the window that is the "active" window in its column.

Contrary to `is-active`, there is always one `is-active-in-column` window in each column.
It is the window that was last focused in the column, i.e. the one that will gain focus if this column is focused.

```kdl
window-rule {
    match is-active-in-column=true
}
```

#### `is-floating`

<sup>Since: next release</sup>

Can be `true` or `false`.
Matches floating windows.

> [!NOTE]
> This matcher will apply only after the window is already open.
> This means that you cannot use it to change the window opening properties like `default-window-height` or `open-on-workspace`.

```kdl
window-rule {
    match is-floating=true
}
```

#### `at-startup`

<sup>Since: 0.1.6</sup>

Can be `true` or `false`.
Matches during the first 60 seconds after starting niri.

This is useful for properties like `open-on-output` which you may want to apply only right after starting niri.

```kdl
// Open windows on the HDMI-A-1 monitor at niri startup, but not afterwards.
window-rule {
    match at-startup=true
    open-on-output "HDMI-A-1"
}
```

### Window Opening Properties

These properties apply once, when a window first opens.

To be precise, they apply at the point when niri sends the initial configure request to the window.

#### `default-column-width`

Set the default width for the new window.

This works for floating windows too, despite the word "column" in the name.

```kdl
// Give Blender and GIMP some guaranteed width on opening.
window-rule {
    match app-id="^blender$"

    // GIMP app ID contains the version like "gimp-2.99",
    // so we only match the beginning (with ^) and not the end.
    match app-id="^gimp"

    default-column-width { fixed 1200; }
}
```

#### `default-window-height`

<sup>Since: next release</sup>

Set the default height for the new window.

```kdl
// Open the Firefox picture-in-picture window as floating with 480×270 size.
window-rule {
    match app-id="firefox$" title="^Picture-in-Picture$"

    open-floating true
    default-column-width { fixed 480; }
    default-window-height { fixed 270; }
}
```

#### `open-on-output`

Make the window open on a specific output.

If such an output does not exist, the window will open on the currently focused output as usual.

If the window opens on an output that is not currently focused, the window will not be automatically focused.

```kdl
// Open Firefox and Telegram (but not its Media Viewer)
// on a specific monitor.
window-rule {
    match app-id="firefox$"
    match app-id=r#"^org\.telegram\.desktop$"#
    exclude app-id=r#"^org\.telegram\.desktop$"# title="^Media viewer$"

    open-on-output "HDMI-A-1"
    // Or:
    // open-on-output "Some Company CoolMonitor 1234"
}
```

<sup>Since: 0.1.9</sup> `open-on-output` can now use monitor manufacturer, model, and serial.
Before, it could only use the connector name.

#### `open-on-workspace`

<sup>Since: 0.1.6</sup>

Make the window open on a specific [named workspace](./Configuration:-Named-Workspaces.md).

If such a workspace does not exist, the window will open on the currently focused workspace as usual.

If the window opens on an output that is not currently focused, the window will not be automatically focused.

```kdl
// Open Fractal on the "chat" workspace.
window-rule {
    match app-id=r#"^org\.gnome\.Fractal$"#

    open-on-workspace "chat"
}
```

#### `open-maximized`

Make the window open as a maximized column.

```kdl
// Maximize Firefox by default.
window-rule {
    match app-id="firefox$"

    open-maximized true
}
```

#### `open-fullscreen`

Make the window open fullscreen.

```kdl
window-rule {
    open-fullscreen true
}
```

You can also set this to `false` to *prevent* a window from opening fullscreen.

```kdl
// Make the Telegram media viewer open in windowed mode.
window-rule {
    match app-id=r#"^org\.telegram\.desktop$"# title="^Media viewer$"

    open-fullscreen false
}
```

#### `open-floating`

<sup>Since: next release</sup>

Make the window open in the floating layout.

```kdl
// Open the Firefox picture-in-picture window as floating.
window-rule {
    match app-id="firefox$" title="^Picture-in-Picture$"

    open-floating true
}
```

You can also set this to `false` to *prevent* a window from opening in the floating layout.

```kdl
// Open all windows in the tiling layout, overriding any auto-floating logic.
window-rule {
    open-floating false
}
```

#### `open-focused`

<sup>Since: next release</sup>

Set this to `false` to prevent this window from being automatically focused upon opening.

```kdl
// Don't give focus to the GIMP startup splash screen.
window-rule {
    match app-id="^gimp" title="^GIMP Startup$"

    open-focused false
}
```

You can also set this to `true` to focus the window, even if normally it wouldn't get auto-focused.

```kdl
// Always focus the KeePassXC-Browser unlock dialog.
//
// This dialog opens parented to the KeePassXC window rather than the browser,
// so it doesn't get auto-focused by default.
window-rule {
    match app-id=r#"^org\.keepassxc\.KeePassXC$"# title="^Unlock Database - KeePassXC$"

    open-focused true
}
```

### Dynamic Properties

These properties apply continuously to open windows.

#### `block-out-from`

You can block out windows from xdg-desktop-portal screencasts.
They will be replaced with solid black rectangles.

This can be useful for password managers or messenger windows, etc.
For layer-shell notification pop-ups and the like, you can use a `block-out-from` [layer rule](./Configuration:-Layer-Rules.md).

![Screenshot showing a window visible normally, but blocked out on OBS.](./img/block-out-from-screencast.png)

To preview and set up this rule, check the `preview-render` option in the debug section of the config.

> [!CAUTION]
> The window is **not** blocked out from third-party screenshot tools.
> If you open some screenshot tool with preview while screencasting, blocked out windows **will be visible** on the screencast.

The built-in screenshot UI is not affected by this problem though.
If you open the screenshot UI while screencasting, you will be able to select the area to screenshot while seeing all windows normally, but on a screencast the selection UI will display with windows blocked out.

```kdl
// Block out password managers from screencasts.
window-rule {
    match app-id=r#"^org\.keepassxc\.KeePassXC$"#
    match app-id=r#"^org\.gnome\.World\.Secrets$"#

    block-out-from "screencast"
}
```

Alternatively, you can block out the window out of *all* screen captures, including third-party screenshot tools.
This way you avoid accidentally showing the window on a screencast when opening a third-party screenshot preview.

This setting will still let you use the interactive built-in screenshot UI, but it will block out the window from the fully automatic screenshot actions, such as `screenshot-screen` and `screenshot-window`.
The reasoning is that with an interactive selection, you can make sure that you avoid screenshotting sensitive content.

```kdl
window-rule {
    block-out-from "screen-capture"
}
```

> [!WARNING]
> Be careful when blocking out windows based on a dynamically changing window title.
>
> For example, you might try to block out specific Firefox tabs like this:
>
> ```kdl
> window-rule {
>     // Doesn't quite work! Try to block out the Gmail tab.
>     match app-id="firefox$" title="- Gmail "
>
>     block-out-from "screencast"
> }
> ```
>
> It will work, but when switching from a sensitive tab to a regular tab, the contents of the sensitive tab **will show up on a screencast** for an instant.
>
> This is because window title (and app ID) are not double-buffered in the Wayland protocol, so they are not tied to specific window contents.
> There's no robust way for Firefox to synchronize visibly showing a different tab and changing the window title.

#### `opacity`

Set the opacity of the window.
`0.0` is fully transparent, `1.0` is fully opaque.
This is applied on top of the window's own opacity, so semitransparent windows will become even more transparent.

Opacity is applied to every surface of the window individually, so subsurfaces and pop-up menus will show window content behind them.

![Screenshot showing Adwaita Demo with a semitransparent pop-up menu.](./img/opacity-popup.png)

Also, focus ring and border with background will show through semitransparent windows (see `prefer-no-csd` and the `draw-border-with-background` window rule below).

```kdl
// Make inactive windows semitransparent.
window-rule {
    match is-active=false

    opacity 0.95
}
```

#### `variable-refresh-rate`

<sup>Since: 0.1.9</sup>

If set to true, whenever this window displays on an output with on-demand VRR, it will enable VRR on that output.

```kdl
// Configure some output with on-demand VRR.
output "HDMI-A-1" {
    variable-refresh-rate on-demand=true
}

// Enable on-demand VRR when mpv displays on the output.
window-rule {
    match app-id="^mpv$"

    variable-refresh-rate true
}
```

#### `default-floating-position`

<sup>Since: next release</sup>

Set the initial position for this window when it opens on, or moves to the floating layout.

Afterward, the window will remember its last floating position.

By default, new floating windows open at the center of the screen, and windows from the tiling layout open close to their visual screen position.

The position uses logical coordinates relative to the working area.
By default, they are relative to the top-left corner of the working area, but you can change this by setting `relative-to` to one of these values: `top-left`, `top-right`, `bottom-left`, `bottom-right`.

For example, if you have a bar at the top, then `x=0 y=0` will put the top-left corner of the window directly below the bar.
If instead you write `x=0 y=0 relative-to="top-right"`, then the top-right corner of the window will align with the top-right corner of the workspace, also directly below the bar.

The coordinates change direction based on `relative-to`.
For example, by default (top-left), `x=100 y=200` will put the window 100 pixels to the right and 200 pixels down from the top-left corner.
If you use `x=100 y=200 relative-to="bottom-left"`, it will put the window 100 pixels to the right and 200 pixels *up* from the bottom-left corner.

```kdl
// Open the Firefox picture-in-picture window at the bottom-left corner of the screen
// with a small gap.
window-rule {
    match app-id="firefox$" title="^Picture-in-Picture$"

    default-floating-position x=32 y=32 relative-to="bottom-left"
}
```

#### `draw-border-with-background`

Override whether the border and the focus ring draw with a background.

Set this to `true` to draw them as solid colored rectangles even for windows which agreed to omit their client-side decorations.
Set this to `false` to draw them as borders around the window even for windows which use client-side decorations.

This property can be useful for rectangular windows that do not support the xdg-decoration protocol.

| With Background                                  | Without Background                                  |
| ------------------------------------------------ | --------------------------------------------------- |
| ![](./img/simple-egl-border-with-background.png) | ![](./img/simple-egl-border-without-background.png) |

```kdl
window-rule {
    draw-border-with-background false
}
```

#### `focus-ring` and `border`

<sup>Since: 0.1.6</sup>

Override the focus ring and border options for the window.

These rules have the same options as the normal focus ring and border config in the [layout](./Configuration:-Layout.md) section, so check the documentation there.

However, in addition to `off` to disable the border/focus ring, this window rule has an `on` flag that enables the border/focus ring for the window even if it was otherwise disabled.
The `on` flag has precedence over the `off` flag, in case both are set.

```kdl
window-rule {
    focus-ring {
        off
        width 2
    }
}

window-rule {
    border {
        on
        width 8
    }
}
```

#### `geometry-corner-radius`

<sup>Since: 0.1.6</sup>

Set the corner radius of the window.

On its own, this setting will only affect the border and the focus ring—they will round their corners to match the geometry corner radius.
If you'd like to force-round the corners of the window itself, set `clip-to-geometry true` in addition to this setting.

```kdl
window-rule {
    geometry-corner-radius 12
}
```

The radius is set in logical pixels, and controls the radius of the window itself, that is, the inner radius of the border:

![](./img/geometry-corner-radius.png)

Instead of one radius, you can set four, for each corner.
The order is the same as in CSS: top-left, top-right, bottom-right, bottom-left.

```kdl
window-rule {
    geometry-corner-radius 8 8 0 0
}
```

This way, you can match GTK 3 applications which have square bottom corners:

![](./img/different-corner-radius.png)

#### `clip-to-geometry`

<sup>Since: 0.1.6</sup>

Clips the window to its visual geometry.

This will cut out any client-side window shadows, and also round window corners according to `geometry-corner-radius`.

![](./img/clip-to-geometry.png)

```kdl
window-rule {
    clip-to-geometry true
}
```

Enable border, set `geometry-corner-radius` and `clip-to-geometry`, and you've got a classic setup:

![](./img/border-radius-clip.png)

```kdl
prefer-no-csd

layout {
    focus-ring {
        off
    }

    border {
        width 2
    }
}

window-rule {
    geometry-corner-radius 12
    clip-to-geometry true
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

```kdl
window-rule {
    min-width 100
    max-width 200
    min-height 300
    max-height 300
}
```

```kdl
// Fix OBS with server-side decorations missing a minimum width.
window-rule {
    match app-id=r#"^com\.obsproject\.Studio$"#

    min-width 876
}
```
