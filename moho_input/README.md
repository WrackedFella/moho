# moho_input

Shared input mapping utilities for Moho. Provides consistent key code translation between `winit` physical keys and the internal binding system used throughout the engine.

## Purpose

This crate exists to ensure that both the main binary and `moho_ui` use identical key mappings. Without this shared crate, each would need to duplicate the mapping logic, creating potential inconsistencies.

## Core Function

### `physical_key_to_binding_code`

Converts `winit::keyboard::PhysicalKey` to an internal `u32` binding code.

```rust
use moho_input::physical_key_to_binding_code;
use winit::keyboard::{KeyCode, PhysicalKey};

let key = PhysicalKey::Code(KeyCode::KeyW);
let code = physical_key_to_binding_code(key);
assert_eq!(code, 'W' as u32);
```

## Mapping Scheme

- **Letters (A-Z)**: Mapped to ASCII character codes (`'A'` = 65, `'Z'` = 90)
- **Space**: Mapped to ASCII space (`' '` = 32)
- **Arrow Keys**: Mapped to 0x100-0x103 range
  - Up: 0x100
  - Down: 0x101
  - Left: 0x102
  - Right: 0x103
- **Special Keys**: Mapped to 0x200+ range
  - Escape: 0x200
  - Tab: 0x201
  - Backspace: 0x202
  - Enter: 0x203
  - Shift: 0x204
  - Control: 0x205
  - Alt: 0x206
- **Function Keys (F1-F12)**: Mapped to 0x300-0x30B range
- **Unmapped Keys**: Return 0

## Usage in Other Crates

**In moho_ui** (keybind capture):
```rust
use moho_input::physical_key_to_binding_code;

// User presses a key, capture it for binding
if let PhysicalKey::Code(key_code) = event.physical_key {
    let binding_code = physical_key_to_binding_code(PhysicalKey::Code(key_code));
    if binding_code != 0 {
        // Save this binding
    }
}
```

**In main binary** (input routing):
```rust
use moho_input::physical_key_to_binding_code;

// Check if pressed key matches a bound action
let code = physical_key_to_binding_code(event.physical_key);
if code == settings.forward_key {
    // Handle forward movement
}
```

## Why This Design?

The `u32` binding code scheme allows:
1. Fast equality comparisons (no string parsing)
2. Serialization to configuration files as simple integers
3. Range-based grouping (letters, arrows, special keys)
4. Extension to gamepad buttons (0x400+ range reserved)

## Testing

Run tests with:
```bash
cargo test --package moho_input
```

Tests verify that common keys (letters, space, arrows, special keys) map correctly and consistently.
