These are some of the general principles for the design of niri's window layout.
They can be sidestepped in specific circumstances if there's a good reason.

1. Opening a new window should not affect the sizes of any existing windows.
1. The focused window should not move around on its own.
    - In particular: windows opening, closing, and resizing to the left of the focused window should not cause it to visually move.
1. Actions should apply immediately.
    - Things like resizing or consuming into column take effect immediately, even if the window needs time to catch up.
    - This is important both for compositor responsiveness and predictability, and for keeping the code sane and free of edge cases and unnecessary asynchrony.
1. If a window or popup is larger than the screen, it should be aligned in the top left corner.
    - The top left area of a window is more likely to contain something important, so it should always be visible.
1. Setting window width or height to a fixed pixel size (e.g. `set-column-width 1280` or `default-column-width { fixed 1280; }`) will set the size of the window itself, however setting to a proportional size (e.g. `set-column-width 50%`) will set the size of the tile, including the border added by niri.
    - With proportions, the user is looking to tile multiple windows on the screen, so they should include borders.
    - With fixed sizes, the user wants to test a specific client size or take a specifically sized screenshot, so they should affect the window directly.
    - After the size is set, it is always converted to a value that includes the borders, to make the code sane. That is, `set-column-width 1000` followed by changing the niri border width will resize the window accordingly.

And here are some more principles I try to follow throughout niri.

1. When disabled, eye-candy features should not affect the performance.
    - Things like animations and custom shaders do not run and are not present in the render tree when disabled. Extra offscreen rendering is avoided.
    - Animations specifically are still "started" even when disabled, but with a duration of 0 (this way, they end as soon as the time is advanced). This does not impact performance, but helps avoid a lot of edge cases in the code.
1. Eye-candy features should not cause unreasonable excessive rendering.
    - For example, clip-to-geometry will prevent direct scanout in many cases (since the window surface is not completely visible). But in the cases where the surface or the subsurface *is* completely visible (fully within the clipped region), it will still allow for direct scanout.
    - For example, animations *can* cause damage and even draw to an offscreen every frame, because they are expected to be short (and can be disabled). However, something like the rounded corners shader should not offscreen or cause excessive damage every frame, because it is long-running and constantly active.
1. Be mindful of invisible state.

    This is niri state that is not immediately apparent from looking at the screen. This is not bad per se, but you should carefully consider how to reduce the surprise factor.

    - For example, when a monitor disconnects, all its workspaces move to another connected monitor. In order to be able to restore these workspaces when the first monitor connects again, these workspaces keep the knowledge of which was their *original monitor*—this is an example of invisible state, since you can't tell it in any way by looking at the screen. This can have surprising consequences: imagine disconnecting a monitor at home, going to work, completely rearranging the windows there, then coming back home, and suddenly some random workspaces end up on your home monitor. In order to reduce this surprise factor, whenever a new window appears on a workspace, that workspace resets its *original monitor* to its current monitor. This way, the workspaces you actively worked on remain where they were.
    - For example, niri preserves the view position whenever a window appears, or whenever a window goes full-screen, to restore it afterward. This way, dealing with temporary things like dialogs opening and closing, or toggling full-screen, becomes less annoying, since it doesn't mess up the view position. This is also invisible state, as you cannot tell by looking at the screen where closing a window will restore the view position. If taken to the extreme (previous view position saved forever for every open window), this can be surprising, as closing long-running windows would result in the view shifting around pretty much randomly. To reduce this surprise factor, niri remembers only one last view position per workspace, and forgets this stored view position upon window focus change.
