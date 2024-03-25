### Overview

In this section you can configure input devices like keyboard and mouse, and some input-related options.

All settings at a glance:

```
input {
    keyboard {
        xkb {
            // layout "us"
            // variant "colemak_dh_ortho"
            // options "compose:ralt,ctrl:nocaps"
            // model ""
            // rules ""
        }

        // repeat-delay 600
        // repeat-rate 25
        // track-layout "global"
    }

    touchpad {
        tap
        // dwt
        // dwtp
        natural-scroll
        // accel-speed 0.2
        // accel-profile "flat"
        // tap-button-map "left-middle-right"
        // click-method "clickfinger"
    }

    mouse {
        // natural-scroll
        // accel-speed 0.2
        // accel-profile "flat"
    }

    trackpoint {
        // natural-scroll
        // accel-speed 0.2
        // accel-profile "flat"
    }

    tablet {
        map-to-output "eDP-1"
    }

    touch {
        map-to-output "eDP-1"
    }

    // disable-power-key-handling
    // warp-mouse-to-focus
    // focus-follows-mouse
    // workspace-auto-back-and-forth
}
```

### Keyboard

#### Layout

In the `xkb` section, you can set layout, variant, options, model and rules.
These are passed directly to libxkbcommon, which is also used by most other Wayland compositors.
See the `xkeyboard-config(7)` manual for more information.

```
input {
    keyboard {
        xkb {
            layout "us"
            variant "colemak_dh_ortho"
            options "compose:ralt,ctrl:nocaps"
        }
    }
}
```

When using multiple layouts, niri can remember the current layout globally (the default) or per-window.
You can control this with the `track-layout` option.

- `global`: layout change is global for all windows.
- `window`: layout is tracked for each window individually.

```
input {
    keyboard {
        track-layout "global"
    }
}
```

#### Repeat

Delay is in milliseconds before the keyboard repeat starts.
Rate is in characters per second.

```
input {
    keyboard {
        repeat-delay 600
        repeat-rate 25
    }
}
```

### TBD
