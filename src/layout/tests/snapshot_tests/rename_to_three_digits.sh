#!/usr/bin/env bash
# Rename snapshot test files from 2-digit to 3-digit numbering
# Example: 00_ltr_*.rs -> 000_ltr_*.rs

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Renaming snapshot test files to 3-digit numbering..."
echo ""

# Rename files from 00-09 to 000-009
for i in {0..9}; do
    for file in "${i}_"*.rs; do
        if [ -f "$file" ]; then
            new_name="0${file}"
            echo "Renaming: $file -> $new_name"
            git mv "$file" "$new_name" 2>/dev/null || mv "$file" "$new_name"
        fi
    done
done

# Rename files from 10-37 to 010-037
for i in {10..37}; do
    for file in "${i}_"*.rs; do
        if [ -f "$file" ]; then
            new_name="0${file}"
            echo "Renaming: $file -> $new_name"
            git mv "$file" "$new_name" 2>/dev/null || mv "$file" "$new_name"
        fi
    done
done

echo ""
echo "✓ File renaming complete!"
echo ""
echo "Now updating mod.rs references..."

# Update mod.rs to use new 3-digit names
if [ -f "mod.rs" ]; then
    # Create backup
    cp mod.rs mod.rs.backup
    
    # Update path references from 2-digit to 3-digit
    sed -i 's/#\[path = "\([0-9]\)_/#[path = "0\1_/g' mod.rs
    sed -i 's/#\[path = "\([0-9][0-9]\)_/#[path = "0\1_/g' mod.rs
    
    # Update module names from 2-digit to 3-digit
    sed -i 's/mod ltr_\([a-z_]*\);/mod ltr_\1;/g' mod.rs
    
    echo "✓ mod.rs updated!"
    echo ""
    echo "Backup saved as: mod.rs.backup"
else
    echo "⚠ mod.rs not found, skipping module updates"
fi

echo ""
echo "All done! Files renamed:"
ls -1 0*.rs | head -20
echo "..."
