#!/usr/bin/env bash
# Fix all config files to have proper formatting and hotkey bindings

set -e

echo "Fixing all config files..."

for config in 000_spawning_single/manual/*.kdl 010_spawning_multiple/manual/*.kdl; do
    if [ -f "$config" ]; then
        echo "Processing: $config"
        
        # Create a proper config with hotkeys
        cat > "$config" << 'EOF'
// Base config for golden tests

input {
    keyboard {
        xkb {
            layout "us"
        }
    }
}

output "HEADLESS-1" {
    mode "1280x720"
}

layout {
    gaps 0
    
EOF

        # Add RTL or LTR setting based on filename
        if [[ "$config" == *"rtl"* ]]; then
            echo "    right-to-left" >> "$config"
        else
            echo "    // right-to-left false" >> "$config"
        fi
        
        cat >> "$config" << 'EOF'
    
    center-focused-column "never"
    
    preset-column-widths {
        proportion 0.33333
        proportion 0.5
        proportion 0.66667
    }
    
EOF

        # Add default width based on filename
        if [[ "$config" == *"1-3"* ]]; then
            echo "    default-column-width { proportion 0.33333; }" >> "$config"
        elif [[ "$config" == *"1-2"* ]]; then
            echo "    default-column-width { proportion 0.5; }" >> "$config"
        elif [[ "$config" == *"2-3"* ]]; then
            echo "    default-column-width { proportion 0.66667; }" >> "$config"
        fi
        
        cat >> "$config" << 'EOF'
    
    focus-ring {
        off
    }
    
    border {
        width 4
        active-color "#ffc87f"
        inactive-color "#505050"
    }
}

prefer-no-csd

animations {
    off
}

binds {
    Mod+T hotkey-overlay-title="Open a Terminal: alacritty" { spawn "alacritty"; }
    Mod+R hotkey-overlay-title="Resize Column" { switch-preset-column-width; }
    Mod+F hotkey-overlay-title="Maximize Column" { maximize-column; }
    Mod+Q hotkey-overlay-title="Close Window" { close-window; }
    Mod+Shift+E hotkey-overlay-title="Exit Niri" { quit; }
    
    Mod+H { focus-column-left; }
    Mod+L { focus-column-right; }
    Mod+Shift+H { move-column-left; }
    Mod+Shift+L { move-column-right; }
}
EOF
        
    fi
done

echo ""
echo "✓ All config files fixed!"
