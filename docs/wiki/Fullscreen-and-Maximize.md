There are several ways to make a window big on niri: maximizing the column, maximizing the window to edges, and fullscreening the window.
Let's look at their differences.

## Maximized (full-width) columns

Maximizing the column via `maximize-column` (bound to <kbd>Mod</kbd><kbd>F</kbd> by default) expands its width to cover the whole screen.
Maximized columns still leave space for [struts] and [gaps], and can contain multiple windows.
The windows retain their borders.
This is the simplest of the sizing modes, and is equivalent to `proportion 1.0` column width, or `set-column-width "100%"`.

![Screenshot of a maximized column with two windows.](https://github.com/user-attachments/assets/a6c26f32-a712-4899-861c-d58a9a357e0e)

You can make a window open in a maximized column with the [`open-maximized true`](./Configuration:-Window-Rules.md#open-maximized) window rule.

## Windows maximized to edges

You can maximize an individual window via `maximize-window-to-edges`.
This is the same maximize as you can find on other desktop environments and operating systems: it expands a window to the edges of the available screen area.
You will still see your bar, but not struts, gaps, or borders.

Windows are aware of their maximized-to-edges status and generally respond by squaring their corners.
Windows can also control maximizing-to-edges: when you click on the square icon in the window's titlebar, or double-click on the titlebar, the window will request niri to maximize or unmaximize itself.

You can put multiple maximized windows into a [tabbed column](./Tabs.md), but not into a regular column.

![Screenshot of a window maximized to edges.](https://github.com/user-attachments/assets/00eac78c-4ebe-4a29-88b8-e1047a5f09c6)

You can make a window open maximized-to-edges, or prevent a window from maximizing upon opening, with the [`open-maximized-to-edges`](./Configuration:-Window-Rules.md#open-maximized-to-edges) window rule.

## Fullscreen windows

Windows can go fullscreen, usually seen with video players, presentations or games.
You can also force a window to go fullscreen via `fullscreen-window` (bound to <kbd>Mod</kbd><kbd>Shift</kbd><kbd>F</kbd> by default).
Fullscreen windows cover the entire screen.
Similarly to maximize-to-edges, windows are aware of their fullscreen status, and can respond by hiding their titlebars or other parts of the UI.

Niri renders a solid black backdrop behind fullscreen windows.
This backdrop helps match the screen size when the window itself remains too small (e.g. if you try to fullscreen a fixed-size dialog window), which is the behavior [defined by the Wayland protocol](https://wayland.app/protocols/xdg-shell#xdg_toplevel:request:set_fullscreen).

When a fullscreen window is focused and not animating, it will cover floating windows and the top layer-shell layer.
If you want for example your layer-shell notifications or launcher to appear over fullscreen windows, configure the respective tools to put them on the overlay layer-shell layer.

![Screenshot of a fullscreen window.](https://github.com/user-attachments/assets/479abfd1-9857-43ad-95db-8e64d0870948)

You can make a window open fullscreen, or prevent a window from fullscreening upon opening, with the [`open-fullscreen`](./Configuration:-Window-Rules.md#open-fullscreen) window rule.

## Common behaviors across fullscreen and maximize

Fullscreen or maximized-to-edges windows can only be in the scrolling layout.
So if you try to fullscreen or maximize a [floating window](./Floating-Windows.md), it'll move into the scrolling layout.
Then, unfullscreening/unmaximizing will bring it back into the floating layout automatically.

Thanks to scrollable tiling, fullscreen and maximized windows remain a normal participant of the layout: you can scroll left and right from them and see other windows.

![Screenshot of the overview showing a fullscreen window with other windows side by side.](https://github.com/user-attachments/assets/e336ccd2-d967-4e04-aa0a-8c08518623cb)

Fullscreen and maximize-to-edges are both special states that the windows are aware of and can control.
Windows sometimes want to restore their fullscreen or, more frequently, maximized state when they open.
The best opportunity for this is during the *initial configure* sequence when the window tells niri everything it should know before opening the window.
If the window does this, then `open-maximized-to-edges` and `open-fullscreen` window rules have a chance to block or adjust the request.

However, some clients tend to request to be maximized shortly *after* the initial configure sequence, when the niri already sent them the initial size request (sometimes even after showing on screen, resulting in a quick resize right after opening).
From niri's point of view, the window is already open by this point, so if the window does this, then the `open-maximized-to-edges` and `open-fullscreen` window rules don't do anything.


[struts]: ./Configuration:-Layout.md#struts
[gaps]: ./Configuration:-Layout.md#gaps
