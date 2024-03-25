### Loading

Niri will load configuration from `$XDG_CONFIG_HOME/.config/niri/config.kdl` or `~/.config/niri/config.kdl`.
If that file is missing, niri will create it with the contents of [the default configuration file](../resources/default-config.kdl).
Please use the default configuration file as the starting point for your custom configuration.

The configuration is live-reloaded.
Simply edit and save the config file, and your changes will be applied.
This includes key bindings, output settings like mode, window rules, and everything else.

You can run `niri validate` to parse the config and see any errors.

To use a different config file path, pass it in the `--config` or `-c` argument to `niri`.

### Syntax

The config is written in [KDL].
Lines starting with `//` are comments; they are ignored.

Also, you can put `/-` in front of a node to comment out the entire node:

```
/-output "eDP-1" {
    everything inside here
    is ignored
}
```

### Defaults

Omitting most of the sections of the config file will leave you with the default values for that section.
A notable exception is `binds {}`: they do not get filled with defaults, so make sure you do not erase this section.

[KDL]: https://kdl.dev/
