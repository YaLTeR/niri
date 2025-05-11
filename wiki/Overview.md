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
> So, layer-shell surfaces work this way: the *background* and *bottom* layers zoom out and remain on the workspaces, while the *top* and *overlay* layers remain on top of the overview.
>
> Put your bar on the *top* layer.

Drag-and-drop will scroll the workspaces up/down in the overview, and will activate a workspace if you hold it above for a moment.
Combined with the hot corner, this lets you do a mouse-only DnD across workspaces.

https://github.com/user-attachments/assets/5f09c5b7-ff40-462b-8b9c-f1b8073a2cbb

You can also drag-and-drop a window to a new workspace above, below, or in-between existing workspaces.

https://github.com/user-attachments/assets/b76d5349-aa20-4889-ab90-0a51554c789d

### Configuration

See the full documentation for the `overview {}` section [here](./Configuration:-Miscellaneous.md#overview).

You can set the zoom-out level like this:

```kdl
// Make workspaces four times smaller than normal in the overview.
overview {
    zoom 0.25
}
```

To change the color behind the workspaces, use the `backdrop-color` setting:

```kdl
// Make the backdrop light.
overview {
    backdrop-color "#777777"
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

### Backdrop customization

Apart from setting a custom backdrop color like described above, you can also put a layer-shell wallpaper into the backdrop with a [layer rule](./Configuration:-Layer-Rules.md#place-within-backdrop), for example:

```kdl
// Put swaybg inside the overview backdrop.
layer-rule {
    match namespace="^wallpaper$"
    place-within-backdrop true
}
```

This will only work for *background* layer surfaces that ignore exclusive zones (typical for wallpaper tools).

You can run two different wallpaper tools (like swaybg and swww), one for the backdrop and one for the normal workspace background.
This way you could set the backdrop one to a blurred version of the wallpaper for a nice effect.

You can also combine this with a transparent background color if you don't like the wallpaper moving together with workspaces:

```kdl
// Make the wallpaper stationary, rather than moving with workspaces.
layer-rule {
    // This is for swaybg; change for other wallpaper tools.
    // Find the right namespace by running niri msg layers.
    match namespace="^wallpaper$"
    place-within-backdrop true
}

// Set transparent workspace background color.
layout {
    background-color "transparent"
}

// Optionally, disable the workspace shadows in the overview.
overview {
    workspace-shadow {
        off
    }
}
```
