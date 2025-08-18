### How to disable client-side decorations/make windows rectangular?

Uncomment the [`prefer-no-csd` setting](./Configuration:-Miscellaneous.md#prefer-no-csd) at the top level of the config, and then restart your apps.
Then niri will ask windows to omit client-side decorations, and also inform them that they are being tiled (which makes some windows rectangular, even if they cannot omit the decorations).

Note that currently this will prevent edge window resize handles from showing up.
You can still resize windows by holding <kbd>Mod</kbd> and the right mouse button.

### Why are transparent windows tinted? / Why is the border/focus ring showing up through semitransparent windows?

Uncomment the [`prefer-no-csd` setting](./Configuration:-Miscellaneous.md#prefer-no-csd) at the top level of the config, and then restart your apps.
Niri will draw focus rings and borders *around* windows that agree to omit their client-side decorations.

By default, focus ring and border are rendered as a solid background rectangle behind windows.
That is, they will show up through semitransparent windows.
This is because windows using client-side decorations can have an arbitrary shape.

You can also override this behavior with the [`draw-border-with-background` window rule](./Configuration:-Window-Rules.md#draw-border-with-background).

### How to enable rounded corners for all windows?

Put this window rule in your config:

```kdl
window-rule {
    geometry-corner-radius 12
    clip-to-geometry true
}
```

For more information, check the [`geometry-corner-radius` window rule](./Configuration:-Window-Rules.md#geometry-corner-radius).

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

### Why doesn't niri integrate Xwayland like other compositors?

A combination of factors:

- Integrating Xwayland is quite a bit of work, as the compositor needs to implement parts of an X11 window manager.
- You need to appease the X11 ideas of windowing, whereas for niri I want to have the best code for Wayland.
- niri doesn't have a good global coordinate system required by X11.
- You tend to get an endless stream of X11 bugs that take further time and effort away from other tasks.
- There aren't actually that many X11-only clients nowadays, and xwayland-satellite takes perfect care of most of those.
- niri isn't a Big Serious Desktop Environment which Must Support All Use Cases (and is Backed By Some Corporation).

All in all, the situation works out in favor of avoiding Xwayland integration.

Also, in the next release niri will have seamless built-in xwayland-satellite integration, that will solve the big rough edge of having to set it up manually.

Besides, I wouldn't be too surprised if, down the road, xwayland-satellite becomes the standard way of integrating Xwayland into new compositors, since it takes on the bulk of the annoying work, and isolates the compositor from misbehaving clients.

### Can I enable blur behind semitransparent windows?

Not yet, follow/upvote [this issue](https://github.com/YaLTeR/niri/issues/54).

There's also [a PR](https://github.com/YaLTeR/niri/pull/1634) adding blur to niri which you can build and run manually.
Keep in mind that it's an experimental implementation that may have problems and performance concerns.

### Can I make a window sticky/pinned/"always on top", appear on all workspaces?

Not yet, follow/upvote [this issue](https://github.com/YaLTeR/niri/issues/932).

You can emulate this with a script that uses the niri IPC.
For example, [nirius](https://git.sr.ht/~tsdh/nirius) seems to have this feature (`toggle-follow-mode`).

### How do I make the Bitwarden window in Firefox open as floating?

Firefox seems to first open the Bitwarden window with a generic Firefox title, and only later change the window title to Bitwarden, so you can't effectively target it with an `open-floating` window rule.

You'll need to use a script, for example [this one](https://github.com/YaLTeR/niri/discussions/1599) or other ones (search niri issues and discussions for Bitwarden).
