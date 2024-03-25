These are some of the general principles for the design of niri's window layout. They can be sidestepped in specific circumstances if there's a good reason.

1. Opening a new window should not affect the sizes of any existing windows.
2. The focused window should not move around on its own.
    - In particular: windows opening, closing and resizing to the left of the focused window should not cause it to visually move.
3. If a window or popup is larger than the screen, it should be aligned on the top left corner.
    - The top left area of a window is more likely to contain something important so it should always be visible.
4. Setting window width or height to a fixed pixel size (e.g. `set-column-width 1280` or `default-column-width { fixed 1280; }`) will set the size of the window itself, however setting to a proportional size (e.g. `set-column-width 50%`) will set the size of the tile, including the border added by niri.
    - With proportions, the user is looking to tile multiple windows on screen, so they should include borders.
    - With fixed sizes, the user wants to test a specific client size or take a specifically sized screenshot, so they should affect the window directly.
    - After the size is set, it is always converted to a value that includes the borders, to make the code sane. That is, `set-column-width 1000` followed by changing the niri border width will resize the window accordingly.