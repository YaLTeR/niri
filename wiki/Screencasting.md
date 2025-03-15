### Overview

The primary screencasting interface that niri offers is through portals and pipewire.
It is supported by [OBS], Firefox, Chromium, Electron, Telegram, and other apps.
You can screencast both monitors and individual windows.

In order to use it, you need a working D-Bus session, pipewire, `xdg-desktop-portal-gnome`, and [running niri as a session](./Getting-Started.md) (i.e. through `niri-session` or from a display manager).
On widely used distros this should all "just work".

Alternatively, you can use tools that rely on the `wlr-screencopy` protocol, which niri also supports.

There are several features in niri designed for screencasting.
Let's take a look!

### Block out windows

You can block out specific windows from screencasts, replacing them with solid black rectangles.
This can be useful for password managers or messenger windows, etc.

![Screenshot showing a window visible normally, but blocked out on OBS.](./img/block-out-from-screencast.png)

This is controlled through the `block-out-from` window rule, for example:

```kdl
// Block out password managers from screencasts.
window-rule {
    match app-id=r#"^org\.keepassxc\.KeePassXC$"#
    match app-id=r#"^org\.gnome\.World\.Secrets$"#

    block-out-from "screencast"
}
```

You can similarly block out layer surfaces, using a layer rule:

```kdl
// Block out mako notifications from screencasts.
layer-rule {
    match namespace="^notifications$"

    block-out-from "screencast"
}
```

Check [the corresponding wiki section](./Configuration:-Window-Rules.md#block-out-from) for more details and examples.

### Dynamic screencast target

<sup>Since: next release</sup>

Niri provides a special screencast stream that you can change dynamically.
It shows up as "niri Dynamic Cast Target" in the screencast window dialog.

![Screencast dialog showing niri Dynamic Cast Target.](https://github.com/user-attachments/assets/e236ce74-98ec-4f3a-a99b-29ac1ff324dd)

When you select it, it will start as an empty, transparent video stream.
Then, you can use the following binds to change what it shows:

- `set-dynamic-cast-window` to cast the focused window.
- `set-dynamic-cast-monitor` to cast the focused monitor.
- `clear-dynamic-cast-target` to go back to an empty stream.

You can also use these actions from the command line, for example to interactively pick which window to cast:

```sh
$ niri msg action set-dynamic-cast-window --id $(niri msg --json pick-window | jq .id)
```

https://github.com/user-attachments/assets/c617a9d6-7d5e-4f1f-b8cc-9301182d9634

If the cast target disappears (e.g. the target window closes), the stream goes back to empty.

All dynamic casts share the same target, but new ones start out empty until the next time you change it (to avoid surprises and sharing something sensitive by mistake).

### Indicate screencasted windows

<sup>Since: 25.02</sup>

The [`is-window-cast-target=true` window rule](./Configuration:-Window-Rules.md#is-window-cast-target) matches windows targetted by an ongoing window screencast.
You use it with a special border color to clearly indicate screencasted windows.

This also works for windows targetted by dynamic screencasts.
However, it will not work for windows that just happen to be visible in a full-monitor screencast.

![](https://github.com/user-attachments/assets/375b381e-3a87-4e94-8676-44404971d893)

```kdl
// Indicate screencasted windows with red colors.
window-rule {
    match is-window-cast-target=true

    focus-ring {
        active-color "#f38ba8"
        inactive-color "#7d0d2d"
    }

    border {
        inactive-color "#7d0d2d"
    }

    shadow {
        color "#7d0d2d70"
    }

    tab-indicator {
        active-color "#f38ba8"
        inactive-color "#7d0d2d"
    }
}
```

Example:

![Screencasted window indicated with a red border and shadow.](https://github.com/user-attachments/assets/375b381e-3a87-4e94-8676-44404971d893)

[OBS]: https://obsproject.com/
