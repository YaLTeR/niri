#!/usr/bin/env bash
# Update golden files from actual test output

set -e

cd "$(dirname "$0")"

echo "Running tests to capture actual snapshots..."

# For each test, run it and capture the actual (left) output
tests=(
    "spawn_one_third_one_tile"
    "spawn_one_third_two_tiles"
    "spawn_one_third_three_tiles"
    "spawn_one_third_four_tiles"
    "spawn_one_half_one_tile"
    "spawn_one_half_two_tiles"
    "spawn_one_half_three_tiles"
)

for test in "${tests[@]}"; do
    echo "Updating: $test"
    
    # Run the test and extract the actual output (left side of assertion)
    output=$(cargo test --lib "golden_tests::spawning_multiple::${test}" -- --exact 2>&1 || true)
    
    # Extract the snapshot between 'left: "' and the next '"'
    snapshot=$(echo "$output" | grep -oP 'left: "\K[^"]+' | sed 's/\\n/\n/g')
    
    if [ -n "$snapshot" ]; then
        echo "$snapshot" > "src/layout/tests/golden_tests/010_spawning_multiple/golden/${test}.txt"
        echo "  ✓ Updated ${test}.txt"
    else
        echo "  ⚠ No output captured for $test (might already be passing)"
    fi
done

echo ""
echo "Done! Re-running tests to verify..."
cargo test --lib golden_tests::spawning_multiple 2>&1 | tail -5
