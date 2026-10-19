//! Keystrokes as XUI receives them.

use core::num::NonZeroU16;

use crate::message::KeyFlags;

/// Declares [`VirtualKey`] and its raw code lookup from one list.
macro_rules! virtual_keys {
    ($($(#[$meta:meta])* $name:ident = $raw:literal,)*) => {
        /// Virtual key code the dashboard or XUI compares against.
        /// Controller keys are 0x5800 and up; keyboard keys use Windows
        /// virtual key codes.
        #[repr(u16)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum VirtualKey {
            $($(#[$meta])* $name = $raw,)*
        }

        impl VirtualKey {
            /// The key for a raw code, or `None` for a code with no variant.
            pub const fn from_raw(raw: u16) -> Option<Self> {
                match raw {
                    $($raw => Some(Self::$name),)*
                    _ => None,
                }
            }
        }
    };
}

virtual_keys! {
    Backspace = 0x08,
    Tab = 0x09,
    /// Clears an XuiEdit buffer.
    Clear = 0x0C,
    Enter = 0x0D,
    Escape = 0x1B,
    Space = 0x20,
    PageUp = 0x21,
    PageDown = 0x22,
    Left = 0x25,
    Up = 0x26,
    Right = 0x27,
    Down = 0x28,
    LetterA = 0x41,
    LetterB = 0x42,
    LetterX = 0x58,
    LetterY = 0x59,
    Comma = 0xBC,
    Period = 0xBE,
    BracketLeft = 0xDB,
    BracketRight = 0xDD,
    PadA = 0x5800,
    PadB = 0x5801,
    PadX = 0x5802,
    PadY = 0x5803,
    PadRightShoulder = 0x5804,
    PadLeftShoulder = 0x5805,
    PadLeftTrigger = 0x5806,
    PadRightTrigger = 0x5807,
    PadDpadUp = 0x5810,
    PadDpadDown = 0x5811,
    PadDpadLeft = 0x5812,
    PadDpadRight = 0x5813,
    PadStart = 0x5814,
    PadBack = 0x5815,
    PadLeftStickUp = 0x5820,
    PadLeftStickDown = 0x5821,
    PadLeftStickRight = 0x5822,
    PadLeftStickLeft = 0x5823,
    PadRightStickUp = 0x5830,
    PadRightStickDown = 0x5831,
    PadRightStickRight = 0x5832,
    PadRightStickLeft = 0x5833,
    PadRightStickUpLeft = 0x5834,
    PadRightStickUpRight = 0x5835,
    PadRightStickDownRight = 0x5836,
    PadRightStickDownLeft = 0x5837,
}

impl VirtualKey {
    pub const fn raw(self) -> u16 {
        self as u16
    }
}

/// Nonzero key code with no [`VirtualKey`] variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OtherKey(NonZeroU16);

impl OtherKey {
    pub const fn raw(self) -> u16 {
        self.0.get()
    }
}

/// Key code of a keystroke. The dashboard passes every nonzero code to XUI
/// and button PressKey matches any code, so codes without a variant are kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    Virtual(VirtualKey),
    Other(OtherKey),
}

impl Key {
    /// The key for a raw code, or `None` for code 0, which a keystroke
    /// carrying only a character uses.
    pub const fn from_raw(raw: u16) -> Option<Self> {
        if let Some(key) = VirtualKey::from_raw(raw) {
            return Some(Self::Virtual(key));
        }

        match NonZeroU16::new(raw) {
            Some(raw) => Some(Self::Other(OtherKey(raw))),
            None => None,
        }
    }

    pub const fn raw(self) -> u16 {
        match self {
            Self::Virtual(key) => key.raw(),
            Self::Other(key) => key.raw(),
        }
    }

    pub const fn virtual_key(self) -> Option<VirtualKey> {
        match self {
            Self::Virtual(key) => Some(key),
            Self::Other(_) => None,
        }
    }
}

impl From<VirtualKey> for Key {
    fn from(key: VirtualKey) -> Self {
        Self::Virtual(key)
    }
}

impl PartialEq<VirtualKey> for Key {
    fn eq(&self, other: &VirtualKey) -> bool {
        *self == Self::Virtual(*other)
    }
}

/// Controller user, 0 to 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UserIndex(u8);

impl UserIndex {
    pub const COUNT: u8 = 4;

    pub const fn new(index: u8) -> Option<Self> {
        if index < Self::COUNT {
            Some(Self(index))
        } else {
            None
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Transition a keystroke reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyKind {
    Down,
    /// A system generated repeat of a held key, dispatched like a key down.
    Repeat,
    Up,
}

/// One keystroke from the system input queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Keystroke {
    /// `None` when the keystroke carries only a character.
    pub key: Option<Key>,
    pub user: UserIndex,
    pub kind: KeyKind,
    /// UTF-16 code unit the key types, if any.
    pub character: Option<u16>,
    /// Message flags. `REPEAT` is implied by [`KeyKind::Repeat`].
    pub flags: KeyFlags,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_codes_round_trip() {
        assert_eq!(Key::from_raw(0), None);
        assert_eq!(
            Key::from_raw(0x5813),
            Some(Key::Virtual(VirtualKey::PadDpadRight))
        );
        assert_eq!(VirtualKey::from_raw(0x5840), None);

        let other = Key::from_raw(0x5A).unwrap();
        assert_eq!(other.virtual_key(), None);
        assert_eq!(other.raw(), 0x5A);
    }
}
