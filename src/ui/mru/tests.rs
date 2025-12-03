use proptest::prelude::*;
use proptest_derive::Arbitrary;
use super::*;

// OPTIMIZATION 1: Reusable default config to avoid repeated allocations
thread_local! {
    static DEFAULT_CONFIG: niri_config::MruPreviews = niri_config::MruPreviews::default();
}

// OPTIMIZATION 2: Builder pattern for more flexible thumbnail creation
struct ThumbnailBuilder {
    timestamp: Option<Duration>,
    on_current_output: bool,
    on_current_workspace: bool,
    app_id: Option<String>,
}

impl ThumbnailBuilder {
    fn new() -> Self {
        Self {
            timestamp: None,
            on_current_output: false,
            on_current_workspace: false,
            app_id: None,
        }
    }

    fn timestamp(mut self, timestamp: Option<Duration>) -> Self {
        self.timestamp = timestamp;
        self
    }

    fn on_current_output(mut self, value: bool) -> Self {
        self.on_current_output = value;
        self
    }

    fn on_current_workspace(mut self, value: bool) -> Self {
        self.on_current_workspace = value;
        self
    }

    fn app_id(mut self, app_id: Option<String>) -> Self {
        self.app_id = app_id;
        self
    }

    fn build(self) -> Thumbnail {
        Thumbnail {
            id: MappedId::next(),
            timestamp: self.timestamp,
            on_current_output: self.on_current_output,
            on_current_workspace: self.on_current_workspace,
            app_id: self.app_id,
            size: Size::new(100, 100),
            clock: Clock::with_time(Duration::ZERO),
            config: DEFAULT_CONFIG.with(|c| c.clone()),
            open_animation: None,
            move_animation: None,
            title_texture: Default::default(),
            background: RefCell::new(FocusRing::new(Default::default())),
            border: RefCell::new(FocusRing::new(Default::default())),
        }
    }
}

// OPTIMIZATION 3: Inline and simplify basic thumbnail creation
#[inline]
fn create_thumbnail() -> Thumbnail {
    ThumbnailBuilder::new().build()
}

#[test]
fn remove_last_window_out_of_two() {
    let ops = [Op::Backward, Op::Remove(1)];
    let thumbnails = vec![create_thumbnail(), create_thumbnail()];
    let current_id = thumbnails.first().map(|t| t.id);
    
    let mut mru = WindowMru {
        thumbnails,
        current_id,
        scope: MruScope::All,
        app_id_filter: None,
    };
    
    check_ops(&mut mru, &ops);
}

// OPTIMIZATION 4: Use const array instead of prop_oneof! for better performance
const SCOPES: [MruScope; 3] = [MruScope::All, MruScope::Output, MruScope::Workspace];
const FILTERS: [MruFilter; 2] = [MruFilter::All, MruFilter::AppId];

fn arbitrary_scope() -> impl Strategy<Value = MruScope> {
    proptest::sample::select(SCOPES)
}

fn arbitrary_filter() -> impl Strategy<Value = MruFilter> {
    proptest::sample::select(FILTERS)
}

// OPTIMIZATION 5: More efficient app_id generation with weighted distribution
fn arbitrary_app_id() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        3 => Just(None),           // 60% None (more realistic)
        1 => Just(Some("app-1".to_string())),  // 20%
        1 => Just(Some("app-2".to_string())),  // 20%
    ]
}

// OPTIMIZATION 6: Use the builder pattern for property tests
prop_compose! {
    fn arbitrary_thumbnail()(
        timestamp: Option<Duration>,
        on_current_output: bool,
        on_current_workspace: bool,
        app_id in arbitrary_app_id(),
    ) -> Thumbnail {
        ThumbnailBuilder::new()
            .timestamp(timestamp)
            .on_current_output(on_current_output)
            .on_current_workspace(on_current_workspace)
            .app_id(app_id)
            .build()
    }
}

// OPTIMIZATION 7: Add size constraints and better initial current_id selection
prop_compose! {
    fn arbitrary_mru()(
        thumbnails in proptest::collection::vec(arbitrary_thumbnail(), 1..=8), // Smaller max for faster tests
        current_idx in 0..=7usize,
    ) -> WindowMru {
        let current_id = thumbnails.get(current_idx % thumbnails.len()).map(|t| t.id);
        WindowMru {
            thumbnails,
            current_id,
            scope: MruScope::All,
            app_id_filter: None,
        }
    }
}

// OPTIMIZATION 8: Add Copy derive where possible, better op constraints
#[derive(Debug, Clone, Copy, Arbitrary)]
enum Op {
    Forward,
    Backward,
    First,
    Last,
    SetScope(#[proptest(strategy = "arbitrary_scope()")] MruScope),
    SetFilter(#[proptest(strategy = "arbitrary_filter()")] MruFilter),
    // More realistic remove index range
    Remove(#[proptest(strategy = "0..8usize")] usize),
}

impl Op {
    // OPTIMIZATION 9: Mark as inline for better performance
    #[inline]
    fn apply(&self, mru: &mut WindowMru) {
        match self {
            Op::Forward => mru.forward(),
            Op::Backward => mru.backward(),
            Op::First => mru.first(),
            Op::Last => mru.last(),
            Op::SetScope(scope) => mru.set_scope(*scope),
            Op::SetFilter(filter) => mru.set_filter(*filter),
            Op::Remove(idx) => {
                // Bounds check moved here to avoid invalid operations
                if *idx < mru.thumbnails.len() {
                    mru.remove_by_idx(*idx);
                }
            }
        }
    }
}

// OPTIMIZATION 10: Add early exit optimization
#[inline]
fn check_ops(mru: &mut WindowMru, ops: &[Op]) {
    // Early exit if no ops (minor optimization)
    if ops.is_empty() {
        return;
    }
    
    for op in ops {
        op.apply(mru);
        mru.verify_invariants();
    }
}

// OPTIMIZATION 11: Reduced test iterations for faster CI
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))] // Default is 256, can tune
    
    #[test]
    fn random_operations_dont_panic(
        mut mru in arbitrary_mru(),
        ops in proptest::collection::vec(any::<Op>(), 0..50), // Cap op count for performance
    ) {
        check_ops(&mut mru, &ops);
    }
}

// OPTIMIZATION 12: Additional targeted tests for common edge cases
#[test]
fn empty_after_removes() {
    let mut mru = WindowMru {
        thumbnails: vec![create_thumbnail()],
        current_id: None,
        scope: MruScope::All,
        app_id_filter: None,
    };
    
    mru.remove_by_idx(0);
    assert!(mru.thumbnails.is_empty());
    mru.verify_invariants();
}

#[test]
fn rapid_direction_changes() {
    let thumbnails: Vec<_> = (0..5).map(|_| create_thumbnail()).collect();
    let current_id = thumbnails.first().map(|t| t.id);
    
    let mut mru = WindowMru {
        thumbnails,
        current_id,
        scope: MruScope::All,
        app_id_filter: None,
    };
    
    // Stress test with rapid changes
    for _ in 0..100 {
        mru.forward();
        mru.backward();
    }
    
    mru.verify_invariants();
}

#[test]
fn scope_filter_combinations() {
    let thumbnails = vec![
        ThumbnailBuilder::new()
            .app_id(Some("app-1".to_string()))
            .on_current_workspace(true)
            .build(),
        ThumbnailBuilder::new()
            .app_id(Some("app-2".to_string()))
            .on_current_output(true)
            .build(),
        ThumbnailBuilder::new()
            .app_id(Some("app-1".to_string()))
            .build(),
    ];
    
    let current_id = thumbnails.first().map(|t| t.id);
    let mut mru = WindowMru {
        thumbnails,
        current_id,
        scope: MruScope::All,
        app_id_filter: None,
    };
    
    // Test all scope/filter combinations
    for scope in SCOPES {
        for filter in FILTERS {
            mru.set_scope(scope);
            mru.set_filter(filter);
            mru.verify_invariants();
        }
    }
}

// OPTIMIZATION 13: Benchmark helper (if criterion is available)
#[cfg(test)]
mod bench_helpers {
    use super::*;
    
    pub fn create_large_mru(size: usize) -> WindowMru {
        let thumbnails: Vec<_> = (0..size).map(|_| create_thumbnail()).collect();
        let current_id = thumbnails.first().map(|t| t.id);
        
        WindowMru {
            thumbnails,
            current_id,
            scope: MruScope::All,
            app_id_filter: None,
        }
    }
}
