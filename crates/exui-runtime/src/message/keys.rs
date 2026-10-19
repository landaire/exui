//! Key message flags and `XuiKeyToNavDirection` 0x9214c920.

use crate::input::VirtualKey;
use crate::message::KeyData;

/// Message key flags (`mflags`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeyFlags(u32);

impl KeyFlags {
    pub const NONE: Self = Self(0);
    /// Keystroke flag 0x4.
    pub const REPEAT: Self = Self(0x1);
    /// Keystroke flag 0x8, read as shift for Tab.
    pub const SHIFT: Self = Self(0x2);
    /// Keystroke flag 0x20; suppresses navigation.
    pub const NO_NAVIGATION: Self = Self(0x8);
    /// Keystroke flag 0x8000: the key came from the media remote.
    pub const REMOTE: Self = Self(0x10);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Message flags of raw `XINPUT_KEYSTROKE` flags, as
    /// `XuiProcessKeystroke` 0x92150120 derives them.
    pub const fn from_keystroke_flags(raw: u16) -> Self {
        let raw = raw as u32;
        Self(((raw >> 2) & 0xf) | (((raw >> 15) & 1) << 4))
    }
}

/// Result of `XuiKeyToNavDirection`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavKey {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Tab,
}

/// Maps a key to a navigation direction. Keyboard keys count only when
/// `keyboard` is set (the global keyboard-disable bit is clear or the
/// caller forces keyboard keys).
pub fn key_to_nav(key: &KeyData, keyboard: bool) -> Option<NavKey> {
    if key.flags.contains(KeyFlags::NO_NAVIGATION) {
        return None;
    }

    let nav = match key.key.virtual_key() {
        Some(VirtualKey::PadDpadLeft | VirtualKey::PadLeftStickLeft) => NavKey::Left,
        Some(VirtualKey::PadDpadRight | VirtualKey::PadLeftStickRight) => NavKey::Right,
        Some(VirtualKey::PadDpadUp | VirtualKey::PadLeftStickUp) => NavKey::Up,
        Some(VirtualKey::PadDpadDown | VirtualKey::PadLeftStickDown) => NavKey::Down,
        Some(VirtualKey::PadLeftTrigger) => NavKey::PageUp,
        Some(VirtualKey::PadRightTrigger) => NavKey::PageDown,
        Some(VirtualKey::Left) if keyboard => NavKey::Left,
        Some(VirtualKey::Right) if keyboard => NavKey::Right,
        Some(VirtualKey::Up) if keyboard => NavKey::Up,
        Some(VirtualKey::Down) if keyboard => NavKey::Down,
        Some(VirtualKey::PageUp) if keyboard => NavKey::PageUp,
        Some(VirtualKey::PageDown) if keyboard => NavKey::PageDown,
        Some(VirtualKey::Tab) if keyboard => NavKey::Tab,
        _ => return None,
    };

    Some(nav)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keystroke_flags_map_to_message_flags() {
        // KEY_DOWN alone carries no message flags.
        assert_eq!(KeyFlags::from_keystroke_flags(0x0001), KeyFlags::NONE);
        assert_eq!(KeyFlags::from_keystroke_flags(0x0005), KeyFlags::REPEAT);
        assert_eq!(KeyFlags::from_keystroke_flags(0x0009), KeyFlags::SHIFT);
        assert_eq!(
            KeyFlags::from_keystroke_flags(0x0021),
            KeyFlags::NO_NAVIGATION
        );
        assert_eq!(KeyFlags::from_keystroke_flags(0x8001), KeyFlags::REMOTE);
        // Bits 6 to 14 are not forwarded.
        assert_eq!(KeyFlags::from_keystroke_flags(0x7fc2), KeyFlags::NONE);
        assert_eq!(KeyFlags::from_keystroke_flags(0x803d), KeyFlags(0x1f));
    }
}
