### Overview

<sup>Since: 25.01</sup>

Floating windows in niri always show on top of the tiled windows.
The floating layout does not scroll.
Each workspace/monitor has its own floating layout, just like each workspace/monitor has its own tiling layout.

New windows will automatically float if they have a parent (e.g. dialogs) or if they are fixed size (e.g. splash screens).
To change a window between floating and tiling, you can use the `toggle-window-floating` bind or right click while dragging/moving the window.
You can also use the `open-floating true/false` window rule to either force a window to open as floating, or to disable the automatic floating logic.

Use `switch-focus-between-floating-and-tiling` to switch the focus between the two layouts.
When focused on the floating layout, binds (like `focus-column-right`) will operate on the floating window.

You can precisely position a floating window with a command like `niri msg action move-floating-window -x 100 -y 200`.

### Scratchpad

<sup>Since: 25.09</sup>

A scratchpad is a way to quickly hide and show windows, similar to i3's scratchpad feature.
Scratchpad windows are floating windows that can be toggled between hidden and visible states.

To move a window to the scratchpad (hide it), use the `move-scratchpad` action:

```kdl
binds {
    Mod+Shift+Minus { move-scratchpad; }
}
```

To show the most recently hidden scratchpad window, use the `scratchpad-show` action:

```kdl
binds {
    Mod+Minus { scratchpad-show; }
}
```

When you show a scratchpad window, it will appear centered on your current workspace as a floating window.
If you call `scratchpad-show` again while a scratchpad window is focused, it will hide that window.

**Key behaviors:**
- Scratchpad windows are always **floating** when visible
- Scratchpads are **per-workspace**: hidden windows stay on the workspace where they were hidden
- You can have **multiple scratchpad windows** on each workspace
- Windows are shown in **LIFO order** (last hidden, first shown)
- Works with both tiling and floating windows

**Example usage:**

```kdl
binds {
    // Hide the focused window in scratchpad
    Mod+Shift+Minus { move-scratchpad; }

    // Toggle scratchpad visibility
    Mod+Minus { scratchpad-show; }
}
```

**IPC commands:**

```bash
# Move focused window to scratchpad
niri msg action move-scratchpad

# Move specific window by ID to scratchpad
niri msg action move-scratchpad --id 12345

# Show/hide most recently hidden scratchpad
niri msg action scratchpad-show

# Show/hide specific scratchpad window by ID
niri msg action scratchpad-show --id 12345
```

You can use the window ID to toggle specific scratchpad windows, which is useful when you have multiple scratchpad windows and want to show a particular one.
To get window IDs, use `niri msg windows`.

Common use cases include keeping a terminal, music player, or notes app always at hand without cluttering your workspace.
