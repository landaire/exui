//! Millisecond durations and the wrapping tick count of the timers.

/// A duration in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Milliseconds(pub u32);

/// Millisecond tick count (`DashGetTickCount`), wrapping like the system
/// tick count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tick(pub u32);

impl Tick {
    /// Time from `earlier` to `self`, unsigned across a wrap.
    pub fn elapsed_since(self, earlier: Self) -> Milliseconds {
        Milliseconds(self.0.wrapping_sub(earlier.0))
    }

    /// The tick `duration` after `self`, wrapping.
    pub fn wrapping_add(self, duration: Milliseconds) -> Self {
        Self(self.0.wrapping_add(duration.0))
    }

    /// The tick `duration` before `self`, wrapping.
    pub fn wrapping_sub(self, duration: Milliseconds) -> Self {
        Self(self.0.wrapping_sub(duration.0))
    }
}
