Things to keep in mind with layer-shell components (bars, launchers, etc.):

1. Popups (tooltips, popup menus) render on the same layer as the component itself. Put your bar at the top layer, or menus will render below windows.
2. Components on the bottom and background layers will never receive keyboard focus, including for popups. They will however receive pointer focus as expected.
3. When a full-screen window is active and covers the entire screen, it will render above the top layer, and it will be prioritized for keyboard focus. If your launcher uses the top layer, and you try to run it while looking at a full-screen window, it won't show up. Only the overlay layer will show up on top of full-screen windows.