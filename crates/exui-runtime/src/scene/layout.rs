//! Size, position, design capture and Anchor re-layout.

use alloc::vec::Vec;

use exui_core::value::Vector3;

use crate::scene::Anchor;
use crate::scene::ElementFlags;
use crate::scene::ElementId;
use crate::scene::ElementTree;
use crate::scene::Size;
use crate::scene::SizeChange;

/// One `XuiElement_LayoutChildren` in progress.
struct LayoutFrame {
    parent: ElementId,
    parent_size: Size,
    keep_design: bool,
    next: Option<ElementId>,
    /// Scale of the child whose own layout runs before its SetScale.
    pending_scale: Option<(ElementId, Vector3)>,
}

impl ElementTree {
    /// `XuiElement_SetBounds`: clamps to >= 0 and, when the size changed,
    /// records the change and lets layout classes re-anchor their children.
    /// Nested layouts run depth first, in the order the original's
    /// synchronous size messages run them.
    pub fn set_bounds(&mut self, id: ElementId, width: f32, height: f32) {
        if !self.resize(id, width, height) {
            return;
        }

        let mut frames: Vec<LayoutFrame> = Vec::new();
        frames.extend(self.begin_layout(id));
        while let Some(top) = frames.len().checked_sub(1) {
            if let Some((child, scale)) = frames[top].pending_scale.take() {
                self.set_scale_anchored(child, scale);
            }
            let Some(child) = frames[top].next else {
                let parent = frames[top].parent;
                frames.pop();
                if let Some(element) = self.get_mut(parent) {
                    element.flags.set(ElementFlags::LAYOUT_IN_PROGRESS, false);
                }
                continue;
            };
            frames[top].next = self.next_sibling(child);

            let Some((position, size, scale)) =
                self.anchor_child(child, frames[top].parent_size, frames[top].keep_design)
            else {
                continue;
            };
            self.set_position_from_setter(child, position);
            if self.resize(child, size.width, size.height) {
                frames[top].pending_scale = Some((child, scale));
                frames.extend(self.begin_layout(child));
            } else {
                self.set_scale_anchored(child, scale);
            }
        }
    }

    /// Stores a new size and records the change. True when the element
    /// changed size and lays out its children.
    fn resize(&mut self, id: ElementId, width: f32, height: f32) -> bool {
        let Some(element) = self.get_mut(id) else {
            return false;
        };
        let new = Size {
            width: width.max(0.0),
            height: height.max(0.0),
        };
        let old = element.size;
        if old == new {
            return false;
        }

        element.size = new;
        let lays_out = element.traits.lays_out_children;
        self.size_changes.push(SizeChange {
            element: id,
            old,
            new,
        });

        lays_out
    }

    fn begin_layout(&mut self, parent: ElementId) -> Option<LayoutFrame> {
        let element = self.get_mut(parent)?;
        element.flags.set(ElementFlags::LAYOUT_IN_PROGRESS, true);
        let frame = LayoutFrame {
            parent,
            parent_size: element.size,
            keep_design: element.flags.contains(ElementFlags::KEEP_DESIGN_LAYOUT),
            next: element.links.first_child,
            pending_scale: None,
        };

        Some(frame)
    }

    /// `XuiElement_CaptureDesignLayout`, skipped while the parent lays out.
    pub(crate) fn capture_design(&mut self, id: ElementId) {
        let parent_size = match self.parent(id).and_then(|parent| self.get(parent)) {
            Some(parent) if parent.flags.contains(ElementFlags::LAYOUT_IN_PROGRESS) => return,
            Some(parent) => parent.size,
            // [INFERENCE] A detached element has no parent size; attaching
            // captures again.
            None => Size::default(),
        };
        let Some(element) = self.get_mut(id) else {
            return;
        };

        element.design.size = element.size;
        element.design.parent_size = parent_size;
        element.design.position = element.position;
        element.design.scale_x = element.scale.x;
        element.design.scale_y = element.scale.y;
    }

    /// The Position setter (`XuiElementSetPosition`): under a parent that
    /// keeps design layout the position is scaled by parent size over parent
    /// design size; then the design layout is captured.
    pub(crate) fn set_position_from_setter(&mut self, id: ElementId, position: Vector3) {
        let mut position = position;
        if let Some(parent) = self.parent(id).and_then(|parent| self.get(parent))
            && parent.flags.contains(ElementFlags::KEEP_DESIGN_LAYOUT)
        {
            position.x *= parent.size.width / parent.design.size.width;
            position.y *= parent.size.height / parent.design.size.height;
        }

        if let Some(element) = self.get_mut(id) {
            element.position = position;
            element.flags.set(ElementFlags::TRANSFORM_DIRTY, true);
        }
        self.capture_design(id);
    }

    /// `XuiElement_SetScaleAnchored`: under a parent that keeps design
    /// layout, a scale-anchored axis (both edges plus 0x40 or 0x80) is
    /// rescaled by parent size over parent design size.
    pub(crate) fn set_scale_anchored(&mut self, id: ElementId, scale: Vector3) {
        let mut scale = scale;
        if let Some(element) = self.get(id)
            && let Some(parent) = element.links.parent.and_then(|parent| self.get(parent))
            && parent.flags.contains(ElementFlags::KEEP_DESIGN_LAYOUT)
        {
            let anchor = element.anchor.0;
            let scale_x = Anchor::LEFT | Anchor::RIGHT | Anchor::SCALE_X;
            let scale_y = Anchor::TOP | Anchor::BOTTOM | Anchor::SCALE_Y;
            if anchor & scale_x == scale_x {
                scale.x *= parent.size.width / parent.design.size.width;
            }
            if anchor & scale_y == scale_y {
                scale.y *= parent.size.height / parent.design.size.height;
            }
        }

        if let Some(element) = self.get_mut(id) {
            element.scale = scale;
            element.flags.set(ElementFlags::TRANSFORM_DIRTY, true);
        }
    }

    /// `XuiElement_ApplyAnchor` for one child: position, size and scale.
    fn anchor_child(
        &self,
        child: ElementId,
        parent_size: Size,
        keep_design: bool,
    ) -> Option<(Vector3, Size, Vector3)> {
        let element = self.get(child)?;
        let design = element.design;
        let (dx, dy) = if keep_design {
            (0.0, 0.0)
        } else {
            (
                parent_size.width - design.parent_size.width,
                parent_size.height - design.parent_size.height,
            )
        };
        let anchor = element.anchor.0;
        let pivot = element.pivot;
        let mut size = design.size;
        let mut position = design.position;
        let mut scale = element.scale;

        let x = anchor_axis(
            anchor,
            Axis::X,
            dx,
            design.size.width,
            design.position.x,
            design.scale_x,
            scale.x,
            pivot.x,
        );
        let y = anchor_axis(
            anchor,
            Axis::Y,
            dy,
            design.size.height,
            design.position.y,
            design.scale_y,
            scale.y,
            pivot.y,
        );
        (size.width, position.x, scale.x) = x;
        (size.height, position.y, scale.y) = y;

        Some((position, size, scale))
    }
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
}

/// One axis of `XuiElement_ApplyAnchor`: returns (size, position, scale).
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors the per-axis inputs of XuiElement_ApplyAnchor"
)]
fn anchor_axis(
    anchor: u32,
    axis: Axis,
    delta: f32,
    design_size: f32,
    design_position: f32,
    design_scale: f32,
    scale: f32,
    pivot: f32,
) -> (f32, f32, f32) {
    let (both, scale_bit, far, center) = match axis {
        Axis::X => (
            Anchor::LEFT | Anchor::RIGHT,
            Anchor::SCALE_X,
            Anchor::RIGHT,
            Anchor::CENTER_X,
        ),
        Axis::Y => (
            Anchor::TOP | Anchor::BOTTOM,
            Anchor::SCALE_Y,
            Anchor::BOTTOM,
            Anchor::CENTER_Y,
        ),
    };

    let mut size = design_size;
    let mut position = design_position;
    let mut new_scale = scale;
    if anchor & both == both {
        if anchor & scale_bit != 0 {
            let scaled = design_scale * design_size;
            new_scale = if design_size == 0.0 {
                0.0
            } else {
                design_scale * (scaled + delta) / scaled
            };
            if pivot != 0.0 {
                position = design_position + pivot * new_scale - pivot;
            }
        } else {
            size = if scale == 0.0 {
                0.0
            } else {
                design_size + delta / scale
            };
        }
    } else if anchor & far != 0 {
        position = design_position + delta;
    } else if anchor & center != 0 {
        position = design_position + delta * 0.5;
    }

    (size, position, new_scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_right_stretches_by_delta_over_scale() {
        let (size, position, scale) = anchor_axis(
            Anchor::LEFT | Anchor::RIGHT,
            Axis::X,
            100.0,
            50.0,
            10.0,
            2.0,
            2.0,
            0.0,
        );

        assert_eq!((size, position, scale), (100.0, 10.0, 2.0));
    }

    #[test]
    fn left_right_with_scale_bit_rescales_and_moves_by_pivot() {
        let anchor = Anchor::LEFT | Anchor::RIGHT | Anchor::SCALE_X;
        // sw = 1 * 50; scale = 1 * (50 + 50) / 50 = 2; pos = 10 + 5 * 2 - 5.
        let (size, position, scale) = anchor_axis(anchor, Axis::X, 50.0, 50.0, 10.0, 1.0, 1.0, 5.0);

        assert_eq!((size, position, scale), (50.0, 15.0, 2.0));
    }

    #[test]
    fn scale_bit_with_zero_design_size_gives_zero_scale() {
        let anchor = Anchor::LEFT | Anchor::RIGHT | Anchor::SCALE_X;

        assert_eq!(
            anchor_axis(anchor, Axis::X, 50.0, 0.0, 0.0, 1.0, 1.0, 0.0).2,
            0.0
        );
    }

    #[test]
    fn far_edge_moves_and_center_moves_half() {
        assert_eq!(
            anchor_axis(Anchor::BOTTOM, Axis::Y, 40.0, 10.0, 5.0, 1.0, 1.0, 0.0),
            (10.0, 45.0, 1.0)
        );
        assert_eq!(
            anchor_axis(Anchor::CENTER_Y, Axis::Y, 40.0, 10.0, 5.0, 1.0, 1.0, 0.0),
            (10.0, 25.0, 1.0)
        );
        // Right wins over center.
        assert_eq!(
            anchor_axis(
                Anchor::RIGHT | Anchor::CENTER_X,
                Axis::X,
                40.0,
                10.0,
                5.0,
                1.0,
                1.0,
                0.0
            ),
            (10.0, 45.0, 1.0)
        );
        // Left only keeps the design position.
        assert_eq!(
            anchor_axis(Anchor::LEFT, Axis::X, 40.0, 10.0, 5.0, 1.0, 1.0, 0.0),
            (10.0, 5.0, 1.0)
        );
    }

    #[test]
    fn zero_scale_stretch_gives_zero_size() {
        assert_eq!(
            anchor_axis(
                Anchor::TOP | Anchor::BOTTOM,
                Axis::Y,
                40.0,
                10.0,
                5.0,
                1.0,
                0.0,
                0.0
            )
            .0,
            0.0
        );
    }
}
