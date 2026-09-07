# On-demand window geometry (local fork)

`niri msg --json window-geometry --id <window-id>` returns the exact focused
window's current global logical visual rectangle and the logical output layout
from the same compositor callback. It uses rendered placement, including the
client geometry offset; it does not derive tiled positions from cached IPC
layout data and does not emit per-frame window-layout events.

The request fails while locked, in overview/screenshot/MRU/exit UI, during output
layout animations or screen transitions, for a dragged window, or if actual
keyboard focus belongs to another window or a layer-shell surface.
Consumers must handle these errors instead of guessing coordinates. Geometry
is a snapshot, not a lock: validate it around capture and again after input
backend preparation, immediately before dispatch. Never replay an input operation
after failure. A compositor restart is required after installing
this fork update; replacing only the `niri msg` CLI is insufficient.

Regression: `cargo test -p niri live_window_geometry` exercises tiled and moved
floating windows against the real headless compositor's pointer hit-testing.
