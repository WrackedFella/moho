use super::super::key_mapping::key_to_code;
use crate::prefs::Binding;

/// Detect active modifier keys from egui input state and return as a bitfield.
/// Bit 0: Ctrl, Bit 1: Shift, Bit 2: Alt
pub(super) fn detect_active_modifiers(input: &egui::InputState) -> u8 {
    let mut mods: u8 = 0;
    if input.modifiers.ctrl {
        mods |= 1;
    }
    if input.modifiers.shift {
        mods |= 2;
    }
    if input.modifiers.alt {
        mods |= 4;
    }
    mods
}

/// Convert egui modifiers to binding modifier bitfield.
pub(super) fn modifiers_to_bits(modifiers: &egui::Modifiers) -> u8 {
    let mut mods: u8 = 0;
    if modifiers.ctrl {
        mods |= 1;
    }
    if modifiers.shift {
        mods |= 2;
    }
    if modifiers.alt {
        mods |= 4;
    }
    mods
}

/// Create a binding from an egui key and modifiers.
/// Handles pure modifier keys (Ctrl, Shift, Alt) specially — when the key itself
/// is a modifier with no other modifiers active, it becomes a modifier-only binding.
pub(super) fn create_binding_from_key(key: &egui::Key, modifiers: &egui::Modifiers) -> Binding {
    let mut code: u32 = key_to_code(key);
    let mut mods = modifiers_to_bits(modifiers);

    // Pure modifier keys are represented by special codes with mods cleared so
    // that binding comparisons treat "Shift" as its own binding, not "Shift+Shift".
    if code == 0 {
        if mods == 1 {
            code = 0x205; // Ctrl
            mods = 0;
        } else if mods == 2 {
            code = 0x204; // Shift
            mods = 0;
        } else if mods == 4 {
            code = 0x206; // Alt
            mods = 0;
        }
    }

    Binding::new(code, mods)
}
