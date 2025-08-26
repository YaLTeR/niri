## Screen readers

<sup>Since: next release</sup>

Niri has basic support for screen readers (specifically, [Orca](https://orca.gnome.org)).
We implement the `org.freedesktop.a11y.KeyboardMonitor` D-Bus interface for Orca to listen and grab keyboard keys, and we expose the main niri UI elements via [AccessKit](https://accesskit.dev).

Make sure [Xwayland](./Xwayland.md) works, then run `orca`.
The default config binds <kbd>Super</kbd><kbd>Alt</kbd><kbd>S</kbd> to toggle Orca, which is the standard key binding.

If you're shipping niri and would like to make it work better for screen readers out of the box, consider the following changes to the default niri config:

- Change the default terminal from Alacritty to one that supports screen readers. For example, [GNOME Console](https://gitlab.gnome.org/GNOME/console) or [GNOME Terminal](https://gitlab.gnome.org/GNOME/gnome-terminal) should work well.
- Change the default application launcher and screen locker to ones that support screen readers. Suggestions welcome! Likely, something GTK-based will work fine.
- Add some `spawn-at-startup` command that plays a sound which will indicate to users that niri has finished loading.
- Add `spawn-at-startup "orca"` to run Orca automatically at niri startup.

## Desktop zoom

There's no built-in zoom yet, but you can use third-party utilities like [wooz](https://github.com/negrel/wooz).
