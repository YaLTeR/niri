use insta::assert_snapshot;

use super::*;

fn make_options() -> Options {
    let mut options = Options {
        layout: niri_config::Layout {
            // Explicitly set all layout options to known values for comprehensive testing
            gaps: 0.0,
            struts: niri_config::Struts {
                left: niri_config::FloatOrInt(0.0),
                right: niri_config::FloatOrInt(0.0),
                top: niri_config::FloatOrInt(0.0),
                bottom: niri_config::FloatOrInt(0.0),
            },
            center_focused_column: niri_config::CenterFocusedColumn::Never,
            always_center_single_column: false,
            default_column_width: Some(niri_config::PresetSize::Proportion(1.0 / 3.0)),
            preset_column_widths: vec![
                niri_config::PresetSize::Proportion(1.0 / 3.0),
                niri_config::PresetSize::Proportion(1.0 / 2.0),
                niri_config::PresetSize::Proportion(2.0 / 3.0),
            ],
            preset_window_heights: vec![
                niri_config::PresetSize::Proportion(1.0 / 3.0),
                niri_config::PresetSize::Proportion(1.0 / 2.0),
                niri_config::PresetSize::Proportion(2.0 / 3.0),
            ],
            default_column_display: niri_ipc::ColumnDisplay::Normal,
            empty_workspace_above_first: false,
            ..Default::default()
        },
        ..Options::default()
    };
    // Disable animations for these tests to make snapshots deterministic and immediate
    options.animations.window_open.anim.off = true;
    options.animations.window_close.anim.off = true;
    options.animations.window_resize.anim.off = true;
    options.animations.window_movement.0.off = true;
    options.animations.horizontal_view_movement.0.off = true;

    options
}

fn set_up_empty() -> Layout<TestWindow> {
    let ops = [
        Op::AddOutput(1),
    ];
    check_ops_with_options(make_options(), ops)
}

#[test]
fn spawn_one_third_tiles() {
    let mut layout = set_up_empty();

    // 1) spawn a 1/3 tile result [1/3 tile, 2/3 empty]
    let ops = [
        Op::AddWindow {
            params: TestWindowParams::new(1),
        },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    ");

    // 2) spawn a second 1/3 tile [1/3 first tile, 1/3 new tile, 1/3 empty]
    let ops = [
        Op::AddWindow {
            params: TestWindowParams::new(2),
        },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-426.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    ");

    // 3) spawn a third 1/3 tile [1/3 first tile, 1/3 second tile, 1/3 new tile] (workspace full)
    let ops = [
        Op::AddWindow {
            params: TestWindowParams::new(3),
        },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-852.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");

    // 4) spawn a fourth 1/3 tile (workspace full), the first tile is now out of bounds on the left side) did the camera move?
    let ops = [
        Op::AddWindow {
            params: TestWindowParams::new(4),
        },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-854.0)
    Active Column: 3
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 3: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=4
    ");
}

#[test]
fn spawn_one_half_tiles() {
    let mut layout = set_up_empty();

    // 1) spawn a 1/2 tile [1/2 tile, 1/2 empty]
    let ops = [
        Op::AddWindow {
            params: TestWindowParams::new(1),
        },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    ");

    // 2) spawn a second tile [1/2 first tile, 1/2 new tile] (workspace full)
    let ops = [
        Op::AddWindow {
            params: TestWindowParams::new(2),
        },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-640.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    ");

    // 3) spawn a third tile (workspace still full) the first tile is now out of bounds on the left side) did the camera move?
    let ops = [
        Op::AddWindow {
            params: TestWindowParams::new(3),
        },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-640.0)
    Active Column: 2
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn focus_movement() {
    let mut layout = set_up_empty();

    // 1) Spawn 4 windows, each 1/3 width. This will overflow the view.
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(4) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-300.0)
    Active Column: 3
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 3: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=4
    ");

    // 2) Move focus left to Window 3.
    // View offset shouldn't change because Window 3 is already visible.
    let ops = [
        Op::FocusColumnLeft,
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-0.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 3: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=4
    ");

    // 3) Move focus left to Window 1 (which is out of view).
    // View offset should shift to 0 to show Window 1.
    let ops = [
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 3: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=4
    ");

    // 4) Move focus right back to 4.
    // View offset should shift back to show Window 4.
    let ops = [
        Op::FocusColumnRight,
        Op::FocusColumnRight,
        Op::FocusColumnRight,
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-854.0)
    Active Column: 3
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 3: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=4
    ");
}

#[test]
fn window_closing() {
    let mut layout = set_up_empty();

    // 1) Spawn 3 windows (full view)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");

    // 2) Close window 2 (middle).
    // Should have 1 and 3. Focus should go to 3 (next) or 1?
    // Usually niri focuses the next one if available, or previous.
    // Since we closed 2, 3 is next.
    let ops = [
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn column_resize() {
    let mut layout = set_up_empty();

    // 1) Spawn 2 windows, 50% each.
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    ");

    // 2) Resize active column (2) to 1/3.
    // Column 2 becomes 1/3. Column 1 stays 1/2.
    let ops = [
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn move_column() {
    let mut layout = set_up_empty();

    // 1) Spawn 3 windows.
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");

    // 2) Move 3 to index 0 (first).
    // Order should be 3, 1, 2.
    // Active should still be 3 (now at 0).
    // View offset should shift to 0.
    let ops = [
        Op::MoveColumnToIndex(0),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn maximized_fullscreen() {
    let mut layout = set_up_empty();

    // 1) Spawn 2 windows.
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    ");

    // 2) Maximize Window 2.
    // Should fill screen (minus gaps/struts if any).
    // View offset should center it or fit it.
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 1280.0, h: 720.0 }, window_id=2
    ");

    // 3) Unmaximize
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    ");

    // 4) Fullscreen Window 2.
    // Should fill screen completely.
    let ops = [
        Op::SetFullscreenWindow { window: 2, is_fullscreen: true },
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 1280.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn window_stacking() {
    let mut layout = set_up_empty();

    // 1) Spawn 2 windows, then add a third to the first column
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    ");

    // 2) Add a third window to column 0 (consume into column)
    // Focus column 0 first, then add window
    let ops = [
        Op::FocusColumnLeft,
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-640.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    Column 2: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    ");

    // 3) Focus the first window in the column
    let ops = [
        Op::FocusWindowUp,
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-640.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    Column 2: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn consume_expel_windows() {
    let mut layout = set_up_empty();

    // 1) Spawn 3 separate windows in 3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");

    // 2) Consume window 2 into column with window 3 (ConsumeWindowIntoColumn)
    // This should merge the columns
    let ops = [
        Op::ConsumeWindowIntoColumn,
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");

    // 3) Expel window 3 from the column (ExpelWindowFromColumn)
    let ops = [
        Op::ExpelWindowFromColumn,
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn window_height_resize() {
    let mut layout = set_up_empty();

    // 1) Create a column with 2 windows
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    ");

    // 2) Resize the active window to a fixed height
    let ops = [
        Op::SetWindowHeight {
            id: Some(2),
            change: SizeChange::SetFixed(200),
        },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 200.0 }, window_id=2
    ");
}

#[test]
fn center_focused_column_always() {    
    // Override options to use CenterFocusedColumn::Always
    let mut options = make_options();
    options.layout.center_focused_column = CenterFocusedColumn::Always;
    
    let ops = [
        Op::AddOutput(1),
    ];
    let mut layout = check_ops_with_options(options.clone(), ops);

    // 1) Spawn 3 windows
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-427.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");

    // 2) Focus left - should center middle column
    let ops = [
        Op::FocusColumnLeft,
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-427.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn mixed_column_widths() {
    let mut layout = set_up_empty();

    // 1) Create columns with different width types: Fixed and Proportion
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetFixed(200)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetFixed(300)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(200.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 200.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(300.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 300.0, h: 720.0 }, window_id=3
    ");

    // 2) Focus left to the proportional column
    let ops = [
        Op::FocusColumnLeft,
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-0.0)
    Active Column: 1
    Column 0: width=Fixed(200.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 200.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(300.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 300.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn edge_cases_boundaries() {
    let mut layout = set_up_empty();

    // 1) Spawn a very wide column (wider than view)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetFixed(1500)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Fixed(1500.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 1500.0, h: 720.0 }, window_id=1
    ");

    // 2) Add another column - view should adjust
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-640.0)
    Active Column: 1
    Column 0: width=Fixed(1500.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 1500.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    ");

    // 3) Close the wide column - view should adjust back
    let ops = [
        Op::CloseWindow(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-640.0)
    Active Column: 0
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn resize_with_multiple_tiles_open() {
    let mut layout = set_up_empty();

    // 1) Create 3 columns with different widths
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");

    // 2) Resize middle column while it's out of view
    let ops = [
        Op::FocusColumn(1),
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-0.0)
    Active Column: 0
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");

    // 3) Resize to fixed width while scrolled
    let ops = [
        Op::SetColumnWidth(SizeChange::SetFixed(800)),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-0.0)
    Active Column: 0
    Column 0: width=Fixed(800.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn resize_stacked_column() {
    let mut layout = set_up_empty();

    // 1) Create a column with 3 stacked windows
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // 2) Resize the column width - should affect all tiles
    let ops = [
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");

    // 3) Resize individual window height in the stack
    let ops = [
        Op::SetWindowHeight {
            id: Some(2),
            change: SizeChange::SetFixed(500),
        },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 500.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn resize_while_scrolled_overflow() {
    let mut layout = set_up_empty();

    // 1) Create 4 columns causing overflow
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(4) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-300.0)
    Active Column: 3
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 3: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=4
    ");

    // 2) Resize active column to be much larger - view should adjust
    let ops = [
        Op::SetColumnWidth(SizeChange::SetFixed(900)),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-300.0)
    Active Column: 3
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 3: width=Fixed(900.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 900.0, h: 720.0 }, window_id=4
    ");

    // 3) Resize back to smaller - view should readjust
    let ops = [
        Op::SetColumnWidth(SizeChange::SetFixed(200)),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-300.0)
    Active Column: 3
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 3: width=Fixed(200.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 200.0, h: 720.0 }, window_id=4
    ");
}

#[test]
fn resize_adjust_incremental() {
    let mut layout = set_up_empty();

    // 1) Start with a 50% column
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    ");

    // 2) Adjust by +10% (incremental resize)
    let ops = [
        Op::SetColumnWidth(SizeChange::AdjustProportion(10.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.6), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 768.0, h: 720.0 }, window_id=1
    ");

    // 3) Adjust by -20% (should go to 40%)
    let ops = [
        Op::SetColumnWidth(SizeChange::AdjustProportion(-20.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.39999999999999997), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 511.0, h: 720.0 }, window_id=1
    ");

    // 4) Adjust with fixed pixels
    let ops = [
        Op::SetColumnWidth(SizeChange::AdjustFixed(100)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Fixed(612.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 612.0, h: 720.0 }, window_id=1
    ");
}

#[test]
fn preset_width_switch_with_empty_space() {
    let mut layout = set_up_empty();

    // Scenario 1: [1/3 tile, 1/3 tile, 1/3 empty]
    // Make middle tile active, press MOD+R
    // Expected: [1/3 tile, 1/2 tile, 1/6 empty]
    
    // 1) Create 2 tiles at 1/3 width each
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    ");

    // 2) Switch preset width on middle tile (MOD+R equivalent)
    // This should cycle to the next preset width (1/2)
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn preset_width_switch_with_overflow() {
    let mut layout = set_up_empty();

    // Scenario 2: [3x 1/3 tile] (all space filled)
    // Make right tile active, press MOD+R
    // Expected: 1/6 of first tile out of bounds, [1/6 visible, 1/3 tile, 1/2 tile]
    
    // 1) Create 3 tiles at 1/3 width each (fills workspace)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");

    // 2) Switch preset width on right tile (MOD+R equivalent)
    // This should cycle to 1/2, causing first tile to go partially out of view
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn preset_width_switch_cycling() {
    let mut layout = set_up_empty();

    // Test cycling through all preset widths
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    ");

    // Cycle to next preset (1/2)
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    ");

    // Cycle to next preset (2/3)
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.6666666666666665), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 853.0, h: 720.0 }, window_id=1
    ");

    // Cycle back to first preset (1/3)
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333326), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    ");
}

// Animation tests for preset width switching
// These tests verify the left-edge pinning behavior during MOD+R

fn make_anim_options() -> Options {
    use niri_config::animations::{Curve, EasingParams, Kind};
    
    const LINEAR: Kind = Kind::Easing(EasingParams {
        duration_ms: 1000,
        curve: Curve::Linear,
    });

    let mut options = Options {
        layout: niri_config::Layout {
            gaps: 0.0,
            default_column_width: Some(niri_config::PresetSize::Proportion(1.0 / 3.0)),
            ..Default::default()
        },
        ..Options::default()
    };
    // Enable animations for width changes
    options.animations.window_resize.anim.kind = LINEAR;
    options.animations.window_movement.0.kind = LINEAR;
    options.animations.horizontal_view_movement.0.kind = LINEAR;

    options
}

fn format_column_edges(layout: &Layout<TestWindow>) -> String {
    use std::fmt::Write as _;
    
    let mut buf = String::new();
    let ws = layout.active_workspace().unwrap();
    let mut tiles: Vec<_> = ws.tiles_with_render_positions().collect();

    tiles.sort_by_key(|(tile, _, _)| tile.window().id());
    for (tile, pos, _visible) in tiles {
        let Size { w, .. } = tile.animated_tile_size();
        let Point { x, .. } = pos;
        let right_edge = x + w;
        writeln!(&mut buf, "left:{x:>4.0} right:{right_edge:>4.0} width:{w:>4.0}").unwrap();
    }
    buf
}

fn set_up_anim_empty() -> Layout<TestWindow> {
    let ops = [Op::AddOutput(1)];
    check_ops_with_options(make_anim_options(), ops)
}

#[test]
fn preset_width_anim_left_edge_pinned() {
    let mut layout = set_up_anim_empty();

    // 1) Create a single 1/3 tile (2/3 empty)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 426 width: 426
    ");

    // 2) Switch to 1/2 width (MOD+R)
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(1),
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // At time=0, still at 1/3 width
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 426 width: 426
    ");

    // Halfway through animation (500ms)
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    // Left edge should stay at 0, right edge should be halfway between 426 and 640
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 533 width: 533
    ");

    // Complete animation (1000ms total)
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    // Final state: 1/2 width (640px), left edge still at 0
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 640 width: 640
    ");

    // 3) Switch to 2/3 width (MOD+R again)
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(1),
    ];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    // Left edge still at 0, right edge halfway between 640 and 853
    assert_snapshot!(format_column_edges(&layout), @"left:   0 right: 747 width: 747");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    // Final state: 2/3 width (853px), left edge still at 0
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 853 width: 853
    ");

    // 4) Switch back to 1/3 width (MOD+R again, completing the cycle)
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(1),
    ];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    // Left edge STILL at 0, right edge shrinking back
    assert_snapshot!(format_column_edges(&layout), @"left:   0 right: 640 width: 640");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    // Back to 1/3 width, left edge NEVER MOVED
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 426 width: 426
    ");
}

#[test]
fn preset_width_anim_with_multiple_columns() {
    let mut layout = set_up_anim_empty();

    // Create 2 columns at 1/3 width each
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:-326 right: 100 width: 426
    left: 100 right: 526 width: 426
    ");

    // Switch second column to 1/2 width
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
    ];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    // First column unchanged, second column's left edge pinned at 426
    assert_snapshot!(format_column_edges(&layout), @r"
    left:-326 right: 100 width: 426
    left: 100 right: 633 width: 533
    ");
    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    // Second column now 1/2 width, left edge still pinned
    assert_snapshot!(format_column_edges(&layout), @r"
    left:-326 right: 100 width: 426
    left: 100 right: 740 width: 640
    ");
}

#[test]
fn preset_width_anim_middle_column_overflow() {
    let mut layout = set_up_anim_empty();

    // Start with 3 columns at 1/3 width each (workspace is full: 426+426+426=1278)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:-652 right:-226 width: 426
    left:-226 right: 200 width: 426
    left: 200 right: 626 width: 426
    ");

    // MOD+R on middle column (to 1/2 width)
    // This pushes the right column partially out of bounds
    let ops = [
        Op::FocusColumn(1),
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
    ];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation (500ms)
    // Middle column growing from 426 to 640 (halfway = 533)
    // Right column shifting right
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:-326 right: 100 width: 426
    left: 100 right: 526 width: 426
    left: 526 right: 952 width: 426
    ");

    // Complete animation
    // Middle now 640px, right column partially out of view (left edge at 1066)
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 426 width: 426
    left: 426 right: 852 width: 426
    left: 852 right:1278 width: 426
    ");

    // MOD+R again on middle column (to 2/3 width)
    // This pushes the right column even more out of bounds
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
    ];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 426 width: 426
    left: 426 right: 852 width: 426
    left: 852 right:1278 width: 426
    ");

    // Complete animation
    // Middle now 853px (2/3), right column mostly out of view
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 426 width: 426
    left: 426 right: 852 width: 426
    left: 852 right:1278 width: 426
    ");

    // MOD+R again on middle column (back to 1/3 width)
    // This brings the right column back into view
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
    ];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    // Middle shrinking from 853 back towards 426
    // Right column coming back into view
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 426 width: 426
    left: 426 right: 852 width: 426
    left: 852 right:1278 width: 426
    ");

    // Complete animation - back to original state
    // All 3 columns at 1/3 width, all visible again
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 426 width: 426
    left: 426 right: 852 width: 426
    left: 852 right:1278 width: 426
    ");
}

#[test]
fn preset_width_anim_rightmost_column_camera() {
    let mut layout = set_up_anim_empty();

    // Start with 3 columns at 1/3 width each, focus on rightmost
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Initial state: all 3 columns visible
    assert_snapshot!(format_column_edges(&layout), @r"
    left:-652 right:-226 width: 426
    left:-226 right: 200 width: 426
    left: 200 right: 626 width: 426
    ");

    // 1st MOD+R: rightmost 1/3 → 1/2 (640px)
    // Camera must adjust to keep rightmost visible, pushing left column partially OOB
    // Expected: 1/6 of first column OOB [1/6 visible, full 1/3 middle, full 1/2 right]
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(3),
    ];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    // Right column growing to 533px (halfway to 640), camera adjusting
    assert_snapshot!(format_column_edges(&layout), @r"
    left:-652 right:-226 width: 426
    left:-226 right: 200 width: 426
    left: 200 right: 733 width: 533
    ");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:-652 right:-226 width: 426
    left:-226 right: 200 width: 426
    left: 200 right: 840 width: 640
    ");

    // 2nd MOD+R: rightmost 1/2 → 2/3 (853px)
    // Camera adjusts more, entire first column goes OOB
    // Expected: [entire 1/3 first OOB, full 1/3 middle, full 2/3 right]
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(3),
    ];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:-652 right:-226 width: 426
    left:-226 right: 200 width: 426
    left: 200 right: 947 width: 747
    ");

    // Complete animation - first column entirely OOB
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:-652 right:-226 width: 426
    left:-226 right: 200 width: 426
    left: 200 right:1053 width: 853
    ");

    // 3rd MOD+R: rightmost 2/3 → 1/3 (back to 426px)
    // Camera adjusts back, but first column stays OOB, creating empty space
    // Expected: [1/3 first OOB, 1/3 middle, 1/3 right, 1/3 empty]
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(3),
    ];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation - right shrinking back
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:-652 right:-226 width: 426
    left:-226 right: 200 width: 426
    left: 200 right: 840 width: 640
    ");

    // Complete animation - back to 1/3, first column still OOB, empty space on right
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:-652 right:-226 width: 426
    left:-226 right: 200 width: 426
    left: 200 right: 626 width: 426
    ");
}

#[test]
fn preset_width_switch_rightmost_column() {
    let mut layout = set_up_empty();

    // Start with 3 columns at 1/3 width each, focus on rightmost
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(3),
    ];
    check_ops_on_layout(&mut layout, ops);

    // Initial state: all three columns visible
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 100 width: 100
    left: 100 right: 200 width: 100
    left: 200 right: 300 width: 100
    ");

    // 1st MOD+R: rightmost 1/3 → 1/2 (640px), camera shifts left
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(3),
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 100 width: 100
    left: 100 right: 200 width: 100
    left: 200 right: 300 width: 100
    ");

    // 2nd MOD+R: rightmost 1/2 → 2/3 (853px), camera shifts further
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(3),
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 100 width: 100
    left: 100 right: 200 width: 100
    left: 200 right: 300 width: 100
    ");

    // 3rd MOD+R: rightmost 2/3 → 1/3 (back to 426px), camera shifts back but left column stays OOB
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(3),
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 100 width: 100
    left: 100 right: 200 width: 100
    left: 200 right: 300 width: 100
    ");
}

// ============================================================================
// Tests for user settings: gaps, struts, center_focused_column, etc.
// ============================================================================

#[test]
fn gaps_between_columns() {
    let mut options = make_options();
    options.layout.gaps = 16.0;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn 3 windows with gaps
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-248.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 405.0, h: 688.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 405.0, h: 688.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 405.0, h: 688.0 }, window_id=3
    ");
}

#[test]
fn struts_left_right() {
    let mut options = make_options();
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(50.0),
        right: niri_config::FloatOrInt(30.0),
        top: niri_config::FloatOrInt(0.0),
        bottom: niri_config::FloatOrInt(0.0),
    };
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn 2 windows with left/right struts
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-150.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 600.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 600.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn struts_top_bottom() {
    let mut options = make_options();
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(0.0),
        right: niri_config::FloatOrInt(0.0),
        top: niri_config::FloatOrInt(40.0),
        bottom: niri_config::FloatOrInt(20.0),
    };
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn a window with top/bottom struts
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 660.0 }, window_id=1
    ");
}

#[test]
fn struts_all_sides() {
    let mut options = make_options();
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(20.0),
        right: niri_config::FloatOrInt(20.0),
        top: niri_config::FloatOrInt(30.0),
        bottom: niri_config::FloatOrInt(30.0),
    };
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn windows with struts on all sides
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-120.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 413.0, h: 660.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 413.0, h: 660.0 }, window_id=2
    ");
}

#[test]
fn center_focused_column_on_overflow() {
    let mut options = make_options();
    options.layout.center_focused_column = CenterFocusedColumn::OnOverflow;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn 3 windows that fit (no overflow)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");

    // 2) Add a 4th window causing overflow - should center
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(4) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-626.0)
    Active Column: 3
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 3: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=4
    ");
}

#[test]
fn always_center_single_column() {
    let mut options = make_options();
    options.layout.always_center_single_column = true;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn a single 1/3 width window - should be centered
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-427.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    ");

    // 2) Add a second window - should no longer center
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-853.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn different_default_column_width() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(0.25));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn windows without explicit width - should use default 25%
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn default_column_width_fixed() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Fixed(400));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn windows - should use fixed 400px default
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn preset_window_heights() {
    let mut layout = set_up_empty();

    // 1) Create a column with 2 stacked windows
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    ");

    // 2) Switch preset window height (should cycle through presets)
    let ops = [
        Op::SwitchPresetWindowHeight { id: Some(2) },
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 240.0 }, window_id=2
    ");

    // 3) Switch again
    let ops = [
        Op::SwitchPresetWindowHeight { id: Some(2) },
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 360.0 }, window_id=2
    ");
}

#[test]
fn gaps_and_struts_combined() {
    let mut options = make_options();
    options.layout.gaps = 8.0;
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(10.0),
        right: niri_config::FloatOrInt(10.0),
        top: niri_config::FloatOrInt(20.0),
        bottom: niri_config::FloatOrInt(20.0),
    };
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn windows with both gaps and struts
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-126.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 618.0, h: 664.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 618.0, h: 664.0 }, window_id=2
    ");
}

#[test]
fn large_gaps() {
    let mut options = make_options();
    options.layout.gaps = 50.0;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn 3 windows with large gaps
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-350.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 360.0, h: 620.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 360.0, h: 620.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 360.0, h: 620.0 }, window_id=3
    ");
}

// ============================================================================
// Tests for spawning windows between existing columns (AddWindowNextTo)
// ============================================================================

#[test]
fn spawn_window_between_two_columns() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Two 1/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    ");

    // Spawn window 3 next to window 1 (inserts between 1 and 2)
    let ops = [
        Op::AddWindowNextTo {
            params: TestWindowParams::new(3),
            next_to_id: 1,
        },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn spawn_window_between_three_columns() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 1/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Spawn window 4 next to window 2 (inserts between 2 and 3)
    let ops = [
        Op::AddWindowNextTo {
            params: TestWindowParams::new(4),
            next_to_id: 2,
        },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 3
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=4
    Column 3: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn spawn_window_between_with_mixed_sizes() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 1/3, 1/2, 2/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 1/2
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 1/2
        Op::SwitchPresetColumnWidth, // 3 -> 2/3
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Spawn window 4 next to window 2 (between 1/2 and 2/3)
    let ops = [
        Op::AddWindowNextTo {
            params: TestWindowParams::new(4),
            next_to_id: 2,
        },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 3
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333326), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=4
    Column 3: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn spawn_window_between_with_gaps() {
    let mut options = make_options();
    options.layout.gaps = 16.0;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 1/3 columns with gaps
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Spawn window 4 next to window 1 (between 1 and 2)
    let ops = [
        Op::AddWindowNextTo {
            params: TestWindowParams::new(4),
            next_to_id: 1,
        },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-248.0)
    Active Column: 3
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=4
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=2
    Column 3: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=3
    ");
}

#[test]
fn spawn_window_between_with_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 2/5 columns (creates overflow)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // Spawn window 4 next to window 2 (inserts in middle, increases overflow)
    let ops = [
        Op::AddWindowNextTo {
            params: TestWindowParams::new(4),
            next_to_id: 2,
        },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 3
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=4
    Column 3: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn empty_workspace_above_first() {
    let mut options = make_options();
    options.layout.empty_workspace_above_first = true;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn windows - should have empty workspace above
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn gaps_struts_and_centering_combined() {
    let mut options = make_options();
    options.layout.gaps = 16.0;
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(20.0),
        right: niri_config::FloatOrInt(20.0),
        top: niri_config::FloatOrInt(10.0),
        bottom: niri_config::FloatOrInt(10.0),
    };
    options.layout.center_focused_column = CenterFocusedColumn::OnOverflow;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn windows with gaps, struts, and centering
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(4) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-384.0)
    Active Column: 3
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 392.0, h: 668.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 392.0, h: 668.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 392.0, h: 668.0 }, window_id=3
    Column 3: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 392.0, h: 668.0 }, window_id=4
    ");
}

#[test]
fn preset_widths_with_gaps() {
    let mut options = make_options();
    options.layout.gaps = 20.0;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn windows and cycle through preset widths with gaps
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-140.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 680.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 680.0 }, window_id=2
    ");

    // 2) Switch preset width to 1/2
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-140.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 680.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333326), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 399.0, h: 680.0 }, window_id=2
    ");

    // 3) Switch preset width to 2/3
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-140.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 680.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 610.0, h: 680.0 }, window_id=2
    ");
}

// ============================================================================
// Tests for alternative preset sizes: 2/5, 3/5, 4/5
// ============================================================================

#[test]
fn preset_two_fifths_tiles() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Add first 2/5 tile [2/5 tile, 3/5 empty]
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    ");

    // 2) Add second 2/5 tile [2/5 first tile, 2/5 second tile, 1/5 empty]
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    ");

    // 3) Add third 2/5 tile - causes overflow, 1/5 of first tile goes OOB
    //    [1/5 OOB | 1/5 inbounds first tile, 2/5 second tile, 2/5 new tile]
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // 4) Focus left to second column - camera should adjust
    let ops = [
        Op::FocusColumnLeft,
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // 5) Focus left to first column - should be fully visible
    let ops = [
        Op::FocusColumnLeft,
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn preset_two_fifths_with_gaps() {
    let mut options = make_options();
    options.layout.gaps = 16.0;
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Add three 2/5 tiles with gaps
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-248.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=3
    ");
}

#[test]
fn preset_two_fifths_with_struts() {
    let mut options = make_options();
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(50.0),
        right: niri_config::FloatOrInt(50.0),
        top: niri_config::FloatOrInt(20.0),
        bottom: niri_config::FloatOrInt(20.0),
    };
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Add three 2/5 tiles with struts
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-250.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 680.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 680.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 680.0 }, window_id=3
    ");
}

#[test]
fn preset_cycling_two_fifths_to_four_fifths() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Add two 2/5 tiles
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    ");

    // 2) Cycle second column to 3/5
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.4), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 512.0, h: 720.0 }, window_id=2
    ");

    // 3) Cycle second column to 4/5
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.6), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 768.0, h: 720.0 }, window_id=2
    ");

    // 4) Cycle back to 2/5
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.8), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 1024.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn preset_two_fifths_gaps_struts_combined() {
    let mut options = make_options();
    options.layout.gaps = 12.0;
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(30.0),
        right: niri_config::FloatOrInt(30.0),
        top: niri_config::FloatOrInt(15.0),
        bottom: niri_config::FloatOrInt(15.0),
    };
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Add three 2/5 tiles with both gaps and struts
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-266.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 666.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 666.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 666.0 }, window_id=3
    ");

    // 2) Cycle middle column to 3/5 - should push third column more OOB
    let ops = [
        Op::FocusColumnLeft,
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-154.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 666.0 }, window_id=1
    Column 1: width=Proportion(0.4), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 471.0, h: 666.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 666.0 }, window_id=3
    ");
}

#[test]
fn preset_cycling_middle_column_with_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three 2/5 tiles - creates overflow
    // (1/5 OOB [1/5 inbounds first tile, 2/5 second tile, 2/5 third tile])
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // Focus middle column and cycle to 3/5
    // (1/5 OOB [1/5 inbounds first tile, 3/5 middle tile, 1/5 inbounds third tile] 1/5 OOB)
    let ops = [
        Op::FocusColumnLeft,
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.4), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 512.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // Cycle middle column to 4/5
    // (1/5 OOB [1/5 inbounds first tile, 4/5 middle tile] 2/5 OOB third tile)
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.6), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 768.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // Cycle middle column back to 2/5
    // (1/5 OOB [1/5 inbounds first tile, 2/5 middle tile, 2/5 third tile])
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.8), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 1024.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn preset_cycling_rightmost_column_with_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three 2/5 tiles - creates overflow
    // (1/5 OOB [1/5 inbounds first tile, 2/5 second tile, 2/5 third tile])
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // Cycle rightmost column to 3/5
    // (2/5 OOB first tile [2/5 second tile, 3/5 third tile])
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.4), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 512.0, h: 720.0 }, window_id=3
    ");

    // Cycle rightmost column to 4/5
    // (2/5 OOB first tile, 1/5 OOB second tile, [1/5 inbounds second tile, 4/5 third tile])
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.6), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 768.0, h: 720.0 }, window_id=3
    ");

    // Cycle rightmost column back to 2/5
    // (2/5 OOB first tile, 1/5 OOB second tile, [1/5 inbounds second tile, 2/5 third tile, 2/5 empty])
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.8), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 1024.0, h: 720.0 }, window_id=3
    ");
}

// ============================================================================
// Tests for window closing behaviors
// ============================================================================

#[test]
fn closing_rightmost_column_with_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three 2/5 tiles - creates overflow
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // Close rightmost column (3) - focus should move to column 2
    let ops = [
        Op::CloseWindow(3),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn closing_middle_column_with_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three 2/5 tiles
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Focus middle column
    let ops = [
        Op::FocusColumnLeft,
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // Close middle column (2) - focus should move to column 3
    let ops = [
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn closing_leftmost_column_with_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three 2/5 tiles
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Focus leftmost column
    let ops = [
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // Close leftmost column (1) - focus should move to column 2
    let ops = [
        Op::CloseWindow(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn closing_with_gaps() {
    let mut options = make_options();
    options.layout.gaps = 16.0;
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three 2/5 tiles with gaps
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-248.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=3
    ");

    // Close middle column - gaps should remain consistent
    let ops = [
        Op::FocusColumnLeft,
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-132.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=3
    ");
}

#[test]
fn closing_mixed_preset_sizes() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three columns with different sizes
    // Column 1: 2/5, Column 2: 3/5, Column 3: 2/5
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // Make column 2 -> 3/5
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.4), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 512.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // Close the large middle column (3/5) - camera should adjust
    let ops = [
        Op::FocusColumnLeft,
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-0.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn closing_with_gaps_and_struts() {
    let mut options = make_options();
    options.layout.gaps = 12.0;
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(30.0),
        right: niri_config::FloatOrInt(30.0),
        top: niri_config::FloatOrInt(15.0),
        bottom: niri_config::FloatOrInt(15.0),
    };
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three columns with gaps and struts
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-266.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 666.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 666.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 666.0 }, window_id=3
    ");

    // Close rightmost column - gaps and struts should remain correct
    let ops = [
        Op::CloseWindow(3),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-154.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 666.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 666.0 }, window_id=2
    ");
}

// ============================================================================
// Tests for closing with 1/3, 1/2, 2/3 preset sizes
// ============================================================================

#[test]
fn closing_first_of_three_thirds_preset() {
    let options = make_options();
    // Use default 1/3, 1/2, 2/3 presets
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 1/3, 1/2, 2/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 1/2
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 1/2
        Op::SwitchPresetColumnWidth, // 3 -> 2/3
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333326), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=3
    ");

    // Focus and close first column (1/3)
    let ops = [
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::CloseWindow(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333326), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn closing_middle_of_three_thirds_preset() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 1/3, 1/2, 2/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 1/2
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 1/2
        Op::SwitchPresetColumnWidth, // 3 -> 2/3
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Focus and close middle column (1/2)
    let ops = [
        Op::FocusColumnLeft,
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-0.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn closing_last_of_three_thirds_preset() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 1/3, 1/2, 2/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 1/2
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 1/2
        Op::SwitchPresetColumnWidth, // 3 -> 2/3
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Close last column (2/3) - already focused
    let ops = [
        Op::CloseWindow(3),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333326), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    ");
}

// ============================================================================
// Tests for closing with 2/5, 3/5, 4/5 preset sizes
// ============================================================================

#[test]
fn closing_first_of_three_fifths_preset() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 2/5, 3/5, 4/5 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 3/5
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 3/5
        Op::SwitchPresetColumnWidth, // 3 -> 4/5
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.4), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 512.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.6), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 768.0, h: 720.0 }, window_id=3
    ");

    // Focus and close first column (2/5)
    let ops = [
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::CloseWindow(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-0.0)
    Active Column: 0
    Column 0: width=Proportion(0.4), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 512.0, h: 720.0 }, window_id=2
    Column 1: width=Proportion(0.6), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 768.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn closing_middle_of_three_fifths_preset() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 2/5, 3/5, 4/5 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 3/5
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 3/5
        Op::SwitchPresetColumnWidth, // 3 -> 4/5
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Focus and close middle column (3/5)
    let ops = [
        Op::FocusColumnLeft,
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-0.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.6), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 768.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn closing_last_of_three_fifths_preset() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 2/5, 3/5, 4/5 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 3/5
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 3/5
        Op::SwitchPresetColumnWidth, // 3 -> 4/5
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Close last column (4/5) - already focused
    let ops = [
        Op::CloseWindow(3),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.4), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 512.0, h: 720.0 }, window_id=2
    ");
}

// ============================================================================
// Tests for MaximizeColumn (Mod+F) fullwidth behavior
// ============================================================================

#[test]
fn maximize_column_first_of_three() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 1/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Focus first column and maximize it
    let ops = [
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::MaximizeColumn,
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 1280.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // Maximize again - should restore original width
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn maximize_column_middle_of_three() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 1/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Focus middle column and maximize it
    let ops = [
        Op::FocusColumnLeft,
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 1280.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");

    // Maximize again - should restore
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn maximize_column_last_of_three() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 1/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Maximize last column (already focused)
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 1280.0, h: 720.0 }, window_id=3
    ");

    // Maximize again - should restore
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn maximize_column_with_mixed_preset_sizes() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 1/3, 1/2, 2/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 1/2
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 1/2
        Op::SwitchPresetColumnWidth, // 3 -> 2/3
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Maximize middle column (1/2)
    let ops = [
        Op::FocusColumnLeft,
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333326), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 1280.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=3
    ");

    // Restore - should go back to 1/2
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333326), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn maximize_column_with_gaps() {
    let mut options = make_options();
    options.layout.gaps = 16.0;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 1/3 columns with gaps
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Maximize middle column with gaps
    let ops = [
        Op::FocusColumnLeft,
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-16.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 1248.0, h: 688.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=3
    ");

    // Restore
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-16.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 688.0 }, window_id=3
    ");
}

#[test]
fn maximize_column_with_struts() {
    let mut options = make_options();
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(50.0),
        right: niri_config::FloatOrInt(50.0),
        top: niri_config::FloatOrInt(20.0),
        bottom: niri_config::FloatOrInt(20.0),
    };
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 1/3 columns with struts
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Maximize middle column - should respect struts
    let ops = [
        Op::FocusColumnLeft,
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-50.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 680.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 1180.0, h: 680.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 680.0 }, window_id=3
    ");

    // Restore
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-50.0)
    Active Column: 1
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 680.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 680.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 680.0 }, window_id=3
    ");
}
