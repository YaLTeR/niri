// Golden tests for basic single column spawning
//
// Structure:
// - ltr.rs: LTR tests with golden snapshots (immutable reference)
// - rtl.rs: RTL tests that derive expectations from LTR
// - golden/*.txt: Immutable golden reference files

use super::*;

mod ltr;
mod rtl;
