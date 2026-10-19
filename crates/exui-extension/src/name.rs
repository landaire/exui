use core::fmt;

/// Extension name: 1 to 8 characters `a-z`, `0-9`. It names the
/// extension's resources (`ext://<name>/`) and, on the console, the image
/// section that holds its archive.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExtensionName {
    bytes: [u8; 8],
    len: u8,
}

impl ExtensionName {
    pub const fn new(name: &str) -> Option<Self> {
        let source = name.as_bytes();
        if source.is_empty() || source.len() > 8 {
            return None;
        }

        let mut bytes = [0; 8];
        let mut index = 0;
        while index < source.len() {
            let byte = source[index];
            if !byte.is_ascii_lowercase() && !byte.is_ascii_digit() {
                return None;
            }
            bytes[index] = byte;
            index += 1;
        }

        Some(Self {
            bytes,
            len: source.len() as u8,
        })
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..usize::from(self.len)])
            .expect("extension names are ASCII")
    }
}

impl fmt::Display for ExtensionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Debug for ExtensionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}", self.as_str())
    }
}
