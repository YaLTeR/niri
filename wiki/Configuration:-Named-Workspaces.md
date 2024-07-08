### Overview

<sup>Since: 0.1.6</sup>

You can declare named workspaces at the top level of the config:

```kdl
workspace "browser"

workspace "chat" {
    open-on-output "DP-2"
}
```

Contrary to normal dynamic workspaces, named workspaces always exist, even when they have no windows.
Otherwise, they behave like any other workspace: you can move them around, move to a different monitor, and so on.

Actions like `focus-workspace` or `move-column-to-workspace` can refer to workspaces by name.
Also, you can use an `open-on-workspace` window rule to make a window open on a specific named workspace:

```kdl
// Declare a workspace named "chat" that opens on the "DP-2" output.
workspace "chat" {
    open-on-output "DP-2"
}

// Open Fractal on the "chat" workspace at niri startup.
window-rule {
    match at-startup=true app-id=r#"^org\.gnome\.Fractal$"#
    open-on-workspace "chat"
}
```

Named workspaces initially appear in the order they are declared in the config file.
When editing the config while niri is running, newly declared named workspaces will appear at the very top of a monitor.

If you delete some named workspace from the config, the workspace will become normal (unnamed), and if there are no windows on it, it will be removed (as any other normal workspace).
There's no way to give a name to an already existing workspace, but you can simply move windows that you want to a new, empty named workspace.
