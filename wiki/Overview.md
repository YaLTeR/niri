### Overview

<sup>Since: next release</sup>

The Overview is a zoomed-out view of your workspaces and windows.
It lets you see what's going on at a glance, navigate, and drag windows around.

Open it with the `toggle-overview` bind, via the top-left hot corner, or via a touchpad four-finger swipe up.
While in the overview, all keyboard binds keep working, while pointing devices get easier:

- Mouse: left click for interactive-move, right click to scroll a workspace left/right (no holding Mod required).
- Touchpad: two-finger scrolling that matches the normal three-finger gestures.
- Touchscreen: one-finger scrolling, or one-finger long press for interactive move.

> [!TIP]
> The overview needs to draw a background under every workspace.
> So, layer-shell surfaces work this way: the *background* and *bottom* layers zoom out and remain under workspaces, while the *top* and *overlay* layers remain on top of the overview.
>
> Put your bar on the *top* layer.

Drag-and-drop will scroll the workspaces up/down in the overview, and will activate a workspace if you hold it above for a moment.
Combined with the hot corner, this lets you do a mouse-only DnD across workspaces.

https://github.com/user-attachments/assets/5f09c5b7-ff40-462b-8b9c-f1b8073a2cbb

You can also drag-and-drop a window to a new workspace above, below, or in-between existing workspaces.

https://github.com/user-attachments/assets/b76d5349-aa20-4889-ab90-0a51554c789d

### Configuration

Overview settings are in the [`overview`](./Configuration:-Miscellaneous.md#overview) section at the top level of the config.

You can set the zoom-out level like this:

```kdl
// Make workspaces four times smaller than normal in the overview.
overview {
    zoom 0.25
}
```

You can also disable the hot corner:

```kdl
// Disable the hot corners.
gestures {
    hot-corners {
        off
    }
}
```
