### Modularization

<sup>Since: next release</sup>

You can use `include` directives to resolve configuration from multiple files.

```kdl
// gestures.kdl
gestures {
    dnd-edge-view-scroll {
        trigger-width 30
        delay-ms 100
        max-speed 1500
    }

    dnd-edge-workspace-switch {
        trigger-height 50
        delay-ms 100
        max-speed 1500
    }

    hot-corners {
        // off
    }
}
```

```kdl
// binds.kdl
binds {
    Mod+Left { focus-column-left; }
    Super+Alt+L { spawn "swaylock"; }
}
```

```kdl
// layout.kdl
layout {
    gaps 16
}
```

```kdl
// config.kdl
include "layout.kdl"

layer-rule {
    match namespace="waybar"
    match at-startup=true

    // Properties that apply continuously.
    opacity 0.5
}

include "gestures.kdl"
include "binds.kdl"
```

This will result in a final effective configuration of:

```kdl
layout {
    gaps 16
}


layer-rule {
    match namespace="waybar"
    match at-startup=true

    // Properties that apply continuously.
    opacity 0.5
}

gestures {
    dnd-edge-view-scroll {
        trigger-width 30
        delay-ms 100
        max-speed 1500
    }

    dnd-edge-workspace-switch {
        trigger-height 50
        delay-ms 100
        max-speed 1500
    }

    hot-corners {
        // off
    }
}

binds {
    Mod+Left { focus-column-left; }
    Super+Alt+L { spawn "swaylock"; }
}
```

Notice how includes are resolved relative to the position in which they appear.

### Duplication and Merging

Named sections can appear more than once.

```kdl,must-fail
// Any section with a name can appear more than once, but the names must be unique.
workspace "browser" {
    open-on-outtput "DP-1"
}

workspace "development" {
    open-on-output "DP-2"
}

// It is INVALID to have multiple workspaces named "development"
workspace "development" {
    open-on-output "DP-1"
}
```

Some sections will be merged when appearing more than once, with a "last one wins" strategy

```kdl
layout {
    gaps 16
    background-color "#003300"

    focus-ring {
        width 4
        active-color "#7fc8ff"
        inactive-color "#505050"
        urgent-color "#9b0000"
    }
}

layout {
    gaps 5

    focus-ring {
        active-color "#505050"
    }
}
```

This would result the resolved configuration:
```kdl
layout {
    gaps 5
    background-color "#003300"

    focus-ring {
        width 4
        active-color "#505050"
        inactive-color "#505050"
        urgent-color "#9b0000"
    }
}
```

Some sections can appear more than once, but will not be merged:

```kdl
// Window rules are not merged
window-rule {
    open-maximized true
}

window-rule {
    match app-id="Alacritty"
    open-maximized false
}
```

Some sections cannot appear more than once and will fail validation:

```kdl,must-fail
// Environment can only be defined once
environment {
    DISPLAY: ":1"
}

environment {
    DISPLAY: ":0"
}
```