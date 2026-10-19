//! Element timers: `XuiSetTimer`, `XuiKillTimer`, `XuiProcessTimers`
//! (xui-controls.md "Timers").

use alloc::vec::Vec;

use crate::message::LevelHandle;
use crate::scene::ElementId;
use crate::scene::ElementTree;
use crate::time::Milliseconds;
use crate::time::Tick;

/// Timer id passed back in message 0x27.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimerId(pub u32);

impl TimerId {
    /// Id used by scenes, sliders, sounds and edits.
    pub const CONTROL: Self = Self(0xaabb_ccdd);
}

#[derive(Clone, Copy, Debug)]
struct TimerNode {
    owner: LevelHandle,
    id: TimerId,
    period: Milliseconds,
    last: Tick,
    killed: bool,
}

/// The timer list and the millisecond tick it is measured against.
#[derive(Debug, Default)]
pub struct Timers {
    /// Most recently added first, as the original links new nodes at the
    /// head of its list.
    nodes: Vec<TimerNode>,
    clock_ms: f64,
}

impl Timers {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            clock_ms: 0.0,
        }
    }

    /// Millisecond tick, wrapping like the system tick count.
    pub fn now(&self) -> Tick {
        Tick((self.clock_ms as u64) as u32)
    }

    /// Advances the tick by the measured frame time.
    pub fn advance_clock(&mut self, dt_ms: f32) {
        self.clock_ms += f64::from(dt_ms.max(0.0));
    }

    /// `XuiSetTimer` on the canonical element handle.
    pub fn set(&mut self, element: ElementId, id: TimerId, period: Milliseconds) {
        self.set_for(LevelHandle::element(element), id, period);
    }

    /// `XuiSetTimer`: adds a timer or restarts an existing one.
    pub fn set_for(&mut self, owner: LevelHandle, id: TimerId, period: Milliseconds) {
        let now = self.now();
        if let Some(node) = self.find_mut(owner, id) {
            node.killed = false;
            node.period = period;
            node.last = now;
            return;
        }

        self.nodes.insert(
            0,
            TimerNode {
                owner,
                id,
                period,
                last: now,
                killed: false,
            },
        );
    }

    /// `XuiKillTimer` on the canonical element handle. Returns false when
    /// no such timer exists.
    pub fn kill(&mut self, element: ElementId, id: TimerId) -> bool {
        self.kill_for(LevelHandle::element(element), id)
    }

    /// `XuiKillTimer`. Returns false when no such timer exists.
    pub fn kill_for(&mut self, owner: LevelHandle, id: TimerId) -> bool {
        let Some(node) = self.find_mut(owner, id) else {
            return false;
        };

        node.killed = true;
        true
    }

    /// Drops every timer of a destroyed element.
    pub fn remove_element(&mut self, element: ElementId) {
        self.nodes.retain(|node| node.owner.element != element);
    }

    /// Frees killed timers and timers of freed elements, then returns the
    /// timers due this frame, in list order. Each due timer restarts its
    /// period from now; missed periods are not accumulated.
    pub(crate) fn begin_pass(&mut self, tree: &ElementTree) -> Vec<(LevelHandle, TimerId)> {
        // A timer set on an already freed element would otherwise stay
        // forever: `remove_element` ran when the element was freed.
        self.nodes
            .retain(|node| !node.killed && tree.contains(node.owner.element));

        self.nodes
            .iter()
            .map(|node| (node.owner, node.id))
            .collect()
    }

    /// Checks one timer of the pass when the walk reaches it.
    pub(crate) fn fire_if_due(&mut self, owner: LevelHandle, id: TimerId) -> bool {
        let now = self.now();
        let Some(node) = self.find_mut(owner, id) else {
            return false;
        };

        if node.killed || now.elapsed_since(node.last) < node.period {
            return false;
        }

        node.last = now;
        true
    }

    fn find_mut(&mut self, owner: LevelHandle, id: TimerId) -> Option<&mut TimerNode> {
        self.nodes
            .iter_mut()
            .find(|node| node.owner == owner && node.id == id)
    }
}
