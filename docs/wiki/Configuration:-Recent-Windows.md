### Overview

<sup>Since: next release</sup>

In this section you can configure the recent windows switcher (Alt-Tab).

Here is an outline of the available settings and their default values:

```kdl
recent-windows {
    // off
    open-delay-ms 150

    highlight {
        active-color "#999999ff"
        urgent-color "#ff9999ff"
        padding 30
        corner-radius 0
    }

    previews {
        max-height 480
        max-scale 0.5
    }

    binds {
        Alt+Tab         { next-window; }
        Alt+Shift+Tab   { previous-window; }
        Alt+grave       { next-window     filter="app-id"; }
        Alt+Shift+grave { previous-window filter="app-id"; }

        Mod+Tab         { next-window; }
        Mod+Shift+Tab   { previous-window; }
        Mod+grave       { next-window     filter="app-id"; }
        Mod+Shift+grave { previous-window filter="app-id"; }
    }
}
```

`off` disables the recent windows switcher altogether.

### `open-delay-ms`

Delay, in milliseconds, between pressing the Alt-Tab bind and the recent windows switcher visually appearing on screen.

The switcher is delayed by default so that quickly tapping Alt-Tab to switch windows wouldn't cause annoying fullscreen visual changes.

```kdl
recent-windows {
    // Make the switcher appear instantly.
    open-delay-ms 0
}
```

### `highlight`

Controls the highlight behind the focused window preview in the recent windows switcher.

- `active-color`: normal color of the focused window highlight.
- `urgent-color`: color of an urgent focused window highlight, also visible in a darker shade on unfocused windows.
- `padding`: padding of the highlight around the window preview, in logical pixels.
- `corner-radius`: corner radius of the highlight.

```kdl
recent-windows {
    // Round the corners on the highlight.
    highlight {
        corner-radius 14
    }
}
```

### `previews`

Controls the window previews in the switcher.

- `max-scale`: maximum scale of the window previews.
Windows cannot be scaled bigger than this value.
- `max-height`: maximum height of the window previews.
Further limits the size of the previews in order to occupy less space on large monitors.

On smaller monitors, the previews will be primarily limited by `max-scale`, and on larger monitors they will be primarily limited by `max-height`.

The `max-scale` limit is imposed twice: on the final window scale, and on the window height which cannot exceed `monitor height × max scale`.

```kdl
recent-windows {
    // Make the previews smaller to fit more on screen.
    previews {
        max-height 320
    }
}
```

```kdl
recent-windows {
    // Make the previews larger to see the window contents.
    previews {
        max-height 1080
        max-scale 0.75
    }
}
```

### `binds`

Configure binds that open and navigate the recent windows switcher.

The defaults are <kbd>Alt</kbd><kbd>Tab</kbd> / <kbd>Mod</kbd><kbd>Tab</kbd> to switch across all windows, and <kbd>Alt</kbd><kbd>\`</kbd> / <kbd>Mod</kbd><kbd>\`</kbd> to switch between windows of the current application.
Adding <kbd>Shift</kbd> will switch windows backwards.

Adding the recent windows `binds {}` section to your config removes all default binds.
You can copy the ones you need from the summary at the top of this wiki page.

```kdl
recent-windows {
    // Even an empty binds {} section will remove all default binds.
    binds {
    }
}
```

The available actions are `next-window` and `previous-window`.
They can optionally have the following properties:

- `filter="app-id"`: filters the switcher to the windows of the currently selected application, as determined by the Wayland app ID.
- `scope="all"`, `scope="output"`, `scope="workspace"`: sets the pre-selected scope when this bind is used to open the recent windows switcher.

```kdl
recent-windows {
    // Pre-select the "Output" scope when switching windows.
    binds {
        Mod+Tab         { next-window     scope="output"; }
        Mod+Shift+Tab   { previous-window scope="output"; }
        Mod+grave       { next-window     scope="output" filter="app-id"; }
        Mod+Shift+grave { previous-window scope="output" filter="app-id"; }
    }
}
```

The recent windows binds have a precedence over the [normal binds](./Configuration:-Key-Bindings.md), meaning that if you have <kbd>Alt</kbd><kbd>Tab</kbd> bound to something else in the normal binds, the `recent-windows` bind will override it.

All binds in this section must have a modifier key like <kbd>Alt</kbd> or <kbd>Mod</kbd> because the recent windows switcher remains open only while you hold any modifier key.

#### Bindings inside the switcher

When the switcher is open, some hardcoded binds are available:

- <kbd>Escape</kbd> cancels the switcher.
- <kbd>Enter</kbd> closes the switcher confirming the current window.
- <kbd>A</kbd>, <kbd>W</kbd>, <kbd>O</kbd> select a specific scope.
- <kbd>S</kbd> cycles between scopes, as indicated by the panel at the top.
- <kbd>←</kbd>, <kbd>→</kbd>, <kbd>Home</kbd>, <kbd>End</kbd> move the selection directionally.

Additionally, certain regular binds will automatically work in the switcher:

- focus column left/right and their variants: will move the selection left/right inside the switcher.
- focus column first/last: will move the selection to the first or last window.
- close window: will close the window currently focused in the switcher.
- screenshot: will open the screenshot UI.

The way this works is by finding all regular binds corresponding to these actions and taking just the trigger key without modifiers.
For example, if you have <kbd>Mod</kbd><kbd>Shift</kbd><kbd>C</kbd> bound to `close-window`, in the window switcher pressing <kbd>C</kbd> on its own will close the window.

This way we don't need to hardcode things like HJKL directional movements.
If you have, say, Colemak-DH MNEI binds instead, they will work for you in the window switcher (as long as they don't conflict with the hardcoded ones).
