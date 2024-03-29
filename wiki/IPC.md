You can communicate with the running niri instance over an IPC socket.
Check `niri msg --help` for available commands.

The `--json` flag prints the response in JSON, rather than formatted.
For example, `niri msg --json outputs`.

For programmatic access, check the [niri-ipc sub-crate](./niri-ipc/) which defines the types.
The communication over the IPC socket happens in JSON.

> [!TIP]
> If you're getting parsing errors from `niri msg` after upgrading niri, make sure that you've restarted niri itself.
> You might be trying to run a newer `niri msg` against an older `niri` compositor.
