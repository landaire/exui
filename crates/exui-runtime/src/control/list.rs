//! XuiList (0x92166b28), XuiCommonList (0x9216aec0), XuiListItem
//! (0x9215d148), XuiScrollEnd (0x9215d4c8) and XuiScrollBar (0x92161ec0).

use alloc::rc::Rc;
use alloc::vec::Vec;

use exui_core::schema::ClassId;
use exui_core::value::Vector3;

use crate::control::Builtin;
use crate::control::ClassState;
use crate::control::StateFrames;
use crate::control::props;
use crate::message::Body;
use crate::message::Context;
use crate::message::CountKind;
use crate::message::Message;
use crate::message::NavData;
use crate::message::NavDirection;
use crate::message::NavKey;
use crate::message::NavQuery;
use crate::message::NotifyKind;
use crate::message::ScrollEndState;
use crate::message::SourceImageQuery;
use crate::message::SourceTextQuery;
use crate::message::TabStopKind;
use crate::message::key_to_nav;
use crate::scene::Anchor;
use crate::scene::ElementId;
use crate::scene::PropValue;
use crate::skin::SkinObject;

/// XuiListItem Layout of the template, copied to the list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum ListLayout {
    /// Zero-initialized by the list constructor.
    #[default]
    VerticalList = 0,
    HorizontalList = 1,
    /// One item control.
    VerticalSpinner = 2,
    /// One item control.
    HorizontalSpinner = 3,
    /// Filled row by row, scrolls vertically.
    GridByRow = 4,
    /// Filled column by column, scrolls horizontally.
    GridByColumn = 5,
}

impl ListLayout {
    pub fn from_raw(raw: u32) -> Option<Self> {
        let layout = match raw {
            0 => Self::VerticalList,
            1 => Self::HorizontalList,
            2 => Self::VerticalSpinner,
            3 => Self::HorizontalSpinner,
            4 => Self::GridByRow,
            5 => Self::GridByColumn,
            _ => return None,
        };

        Some(layout)
    }

    fn is_horizontal(self) -> bool {
        matches!(
            self,
            Self::HorizontalList | Self::HorizontalSpinner | Self::GridByColumn
        )
    }

    fn is_spinner(self) -> bool {
        matches!(self, Self::VerticalSpinner | Self::HorizontalSpinner)
    }

    fn is_grid(self) -> bool {
        matches!(self, Self::GridByRow | Self::GridByColumn)
    }
}

/// XuiCommonList per-item flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemFlags(pub u32);

impl ItemFlags {
    pub const NONE: Self = Self(0);
    pub const DISABLED: Self = Self(0x1);
    pub const CHECKED: Self = Self(0x2);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn set(&mut self, other: Self, on: bool) {
        if on {
            self.0 |= other.0;
        } else {
            self.0 &= !other.0;
        }
    }
}

/// XuiList instance (0x9c bytes in the original). The margins at
/// +0x54/+0x58 are not kept: they only apply with +0x98 bit2, which no
/// observed code sets. Cheap to clone: the item controls are shared.
#[derive(Clone, Debug, Default)]
pub struct ListState {
    pub checkable: bool,
    pub keep_size_unfocused: bool,
    pub focus_within: bool,
    pub var_h: bool,
    pub var_w: bool,
    pub next_end: Option<ElementId>,
    pub prev_end: Option<ElementId>,
    pub bar: Option<ElementId>,
    pub item_w: f32,
    pub item_h: f32,
    pub item_cur_w: f32,
    pub item_cur_h: f32,
    pub sel_h: f32,
    pub sel_w: f32,
    pub sel_cur_h: f32,
    pub sel_cur_w: f32,
    pub spacing_y: f32,
    pub spacing_x: f32,
    pub template_position: Vector3Copy,
    pub template_anchor: Anchor,
    pub layout: ListLayout,
    pub multi: bool,
    pub template: Option<SkinObject>,
    pub items: Rc<[ElementId]>,
    pub slot: Option<usize>,
    pub rows: u32,
    pub cols: u32,
    pub top: Option<u32>,
    pub cur: Option<u32>,
    pub count: u32,
    pub focus_self_active: bool,
}

/// Copyable position triple.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vector3Copy {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl ListState {
    pub fn new() -> Self {
        Self::default()
    }

    fn per_line(&self) -> u32 {
        if self.layout.is_horizontal() {
            self.rows
        } else {
            self.cols
        }
    }

    fn spinner_wrap(&self, wrap: bool) -> bool {
        self.layout.is_spinner() && wrap
    }
}

/// XuiCommonList item store.
#[derive(Debug, Default)]
pub struct CommonListState {
    pub texts: Vec<Option<Rc<[u16]>>>,
    pub images: Vec<Option<Rc<[u16]>>>,
    pub flags: Vec<ItemFlags>,
    pub nav: Vec<Option<NavData>>,
}

/// XuiScrollBar instance.
#[derive(Clone, Copy, Debug)]
pub struct ScrollBarState {
    pub count: u32,
    pub top: Option<u32>,
    pub visible: u32,
    pub body: Option<ElementId>,
    pub thumb: Option<ElementId>,
}

impl ScrollBarState {
    /// The constructor zeroes the range.
    pub fn new() -> Self {
        Self {
            count: 0,
            top: Some(0),
            visible: 0,
            body: None,
            thumb: None,
        }
    }
}

impl Default for ScrollBarState {
    fn default() -> Self {
        Self::new()
    }
}

fn list<'c>(cx: &'c mut Context<'_, '_>, element: ElementId) -> Option<&'c mut ListState> {
    match cx.class_state(element)? {
        ClassState::List { list, .. } => Some(list),
        _ => None,
    }
}

fn snapshot(cx: &mut Context<'_, '_>, element: ElementId) -> Option<ListState> {
    list(cx, element).map(|state| state.clone())
}

fn wrap(cx: &Context<'_, '_>, element: ElementId) -> bool {
    matches!(
        cx.tree.property(element, &[props::list_wrap()], None),
        Some(PropValue::Bool(true))
    )
}

fn size(cx: &Context<'_, '_>, element: ElementId) -> (f32, f32) {
    cx.tree.get(element).map_or((0.0, 0.0), |element| {
        (element.size().width, element.size().height)
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct ItemRect {
    w: f32,
    h: f32,
    x: f32,
    y: f32,
    z: f32,
}

/// `XuiList_GetItemRect` 0x9215d930.
fn item_rect(state: &ListState, index: usize) -> ItemRect {
    let mut slot = state.slot;
    if (state.var_w || state.var_h)
        && slot.is_none()
        && let (Some(cur), Some(top)) = (state.cur, state.top)
    {
        let distance = i64::from(cur) - i64::from(top);
        if distance >= 0 && (distance as usize) < state.items.len() {
            slot = Some(distance as usize);
        }
    }
    let big = slot.is_some() && (state.focus_within || state.keep_size_unfocused);
    let i = index as f32;
    let p = state.template_position;
    let (w, h) = (state.item_w, state.item_h);
    let (sx, sy) = (state.spacing_x, state.spacing_y);

    let mut rect = ItemRect {
        x: p.x,
        y: p.y,
        z: p.z,
        ..ItemRect::default()
    };
    match state.layout {
        ListLayout::VerticalList => {
            rect.y = if slot.is_some_and(|slot| index > slot) && big {
                (sy + h) * (i - 1.0) + (state.sel_cur_h + sy) + p.y
            } else {
                (sy + h) * i + p.y
            };
            if slot == Some(index) && big {
                rect.x = libm::floorf(p.x - (state.sel_cur_w - state.item_cur_w) * 0.5 + 0.5);
                rect.w = state.sel_cur_w;
                rect.h = state.sel_cur_h;
            } else {
                rect.w = state.item_cur_w;
                rect.h = h;
            }
        }
        ListLayout::HorizontalList => {
            rect.x = if slot.is_some_and(|slot| index > slot) && big {
                (sx + w) * (i - 1.0) + (state.sel_cur_w + sx) + p.x
            } else {
                (sx + w) * i + p.x
            };
            if slot == Some(index) && big {
                rect.y = libm::floorf(p.y - (state.sel_cur_h - state.item_cur_h) * 0.5 + 0.5);
                rect.w = state.sel_cur_w;
                rect.h = state.sel_cur_h;
            } else {
                rect.w = w;
                rect.h = state.item_cur_h;
            }
        }
        ListLayout::VerticalSpinner | ListLayout::HorizontalSpinner => {
            rect.w = state.item_cur_w;
            rect.h = state.item_cur_h;
        }
        ListLayout::GridByRow | ListLayout::GridByColumn => {
            let (column, row) = if state.layout == ListLayout::GridByRow {
                let cols = state.cols.max(1) as usize;
                (index % cols, index / cols)
            } else {
                let rows = state.rows.max(1) as usize;
                (index / rows, index % rows)
            };
            rect.x = column as f32 * w + sx * column as f32 + p.x;
            rect.y = row as f32 * h + sy * row as f32 + p.y;
            rect.w = w;
            rect.h = h;
        }
    }

    rect
}

/// `XuiList_GetVisibleCount` 0x9215e120.
fn visible_count(state: &ListState, list_size: (f32, f32)) -> u32 {
    let ctrls = state.items.len() as u32;
    if !state.var_h && !state.var_w {
        return state.count.min(ctrls);
    }

    for slot in (0..state.items.len()).rev() {
        let rect = item_rect(state, slot);
        if state.layout == ListLayout::VerticalList && rect.y + rect.h <= list_size.1 {
            return slot as u32 + 1;
        }
        if state.layout == ListLayout::HorizontalList && rect.x + rect.w <= list_size.0 {
            return slot as u32 + 1;
        }
    }

    state.count.min(ctrls).min(1)
}

/// `XuiList_GetMaxTop` 0x9215e460.
fn max_top(state: &ListState, list_size: (f32, f32)) -> u32 {
    let ctrls = state.items.len() as i64;
    let count = i64::from(state.count);
    let mut max = count - ctrls;
    if state.layout == ListLayout::VerticalList && state.var_h {
        let lines = libm::floorf(
            (list_size.1 - state.template_position.y - state.sel_cur_h)
                / (state.spacing_y + state.item_h),
        ) as i64;
        max = count - (lines + 1);
    }
    if state.layout == ListLayout::HorizontalList && state.var_w {
        let lines = libm::floorf(
            (list_size.0 - state.template_position.x - state.sel_cur_w)
                / (state.spacing_x + state.item_w),
        ) as i64;
        max = count - (lines + 1);
    }

    let per_line = i64::from(state.per_line());
    if per_line != 0 && max % per_line != 0 {
        max += per_line - max % per_line;
    }

    max.max(0) as u32
}

/// Items from the top item to the last one. The original subtracts with an
/// unset top read as -1, which counts one more than `count`.
fn items_from_top(state: &ListState) -> u64 {
    let count = u64::from(state.count);
    match state.top {
        Some(top) => count.saturating_sub(u64::from(top)),
        None => count + 1,
    }
}

/// `XuiList_IsAtEnd` 0x9215e338.
fn is_at_end(state: &ListState, list_size: (f32, f32)) -> bool {
    let ctrls = state.items.len() as u64;
    let remaining = items_from_top(state);
    if !state.var_h && !state.var_w {
        return ctrls >= remaining;
    }

    let Some(slot) = remaining.checked_sub(1).filter(|slot| *slot < ctrls) else {
        return false;
    };

    let rect = item_rect(state, slot as usize);
    rect.y >= state.template_position.y && rect.y + rect.h <= list_size.1
}

fn cur_slot_valid(state: &ListState, visible: u32) -> bool {
    state.cur.is_some_and(|cur| cur < state.count)
        && state.slot.is_some_and(|slot| (slot as u32) < visible)
}

fn slot_for(cur: Option<u32>, top: Option<u32>, visible: u32) -> Option<usize> {
    let (cur, top) = (cur?, top?);
    let slot = cur.checked_sub(top)?;

    (slot < visible).then_some(slot as usize)
}

fn next_end_state(state: &ListState, wrap: bool, list_size: (f32, f32)) -> ScrollEndState {
    if !is_at_end(state, list_size) || state.spinner_wrap(wrap) {
        ScrollEndState::More
    } else {
        ScrollEndState::Normal
    }
}

impl Context<'_, '_> {
    fn scroll_end_set(&mut self, end: Option<ElementId>, state: ScrollEndState) {
        if let Some(end) = end {
            self.send_body(end, Body::ScrollEndSetState { state });
        }
    }

    fn list_get_u32(&mut self, list: ElementId, body: Body) -> Option<Body> {
        let message = self.send_body(list, body);
        message.handled.then_some(message.body)
    }

    /// `XuiListGetItemCount`: 0x7dc kind 0.
    pub fn list_item_count(&mut self, list: ElementId) -> u32 {
        match self.list_get_u32(
            list,
            Body::GetCount {
                kind: CountKind::Items,
                count: 0,
            },
        ) {
            Some(Body::GetCount { count, .. }) => count,
            _ => 0,
        }
    }

    fn list_item_check(&mut self, list: ElementId, index: u32) -> bool {
        matches!(
            self.send_body(
                list,
                Body::GetItemCheck {
                    index,
                    checked: false
                }
            )
            .body,
            Body::GetItemCheck { checked: true, .. }
        )
    }

    fn list_item_enabled(&mut self, list: ElementId, index: u32) -> bool {
        matches!(
            self.send_body(
                list,
                Body::GetItemEnabled {
                    index,
                    enabled: false
                }
            )
            .body,
            Body::GetItemEnabled { enabled: true, .. }
        )
    }

    /// `XuiListSetCurSel`: 0x7d8.
    pub fn list_set_cur_sel(&mut self, list: ElementId, index: Option<u32>) {
        self.send_body(list, Body::SetCurItem { index });
    }

    /// `XuiListSetTopItem`: 0x7db.
    pub fn list_set_top(&mut self, list: ElementId, top: u32) {
        self.send_body(list, Body::SetTop { top: Some(top) });
    }

    /// `XuiListSetCurSelVisible`.
    pub fn list_set_cur_sel_visible(&mut self, list: ElementId, index: u32) {
        let message = self.send_body(list, Body::SetCurItem { index: Some(index) });
        if !message.handled {
            return;
        }

        let top = match self.send_body(list, Body::GetTop { top: None }).body {
            Body::GetTop { top } => top,
            _ => None,
        };
        let visible = match self
            .send_body(
                list,
                Body::GetCount {
                    kind: CountKind::Visible,
                    count: 0,
                },
            )
            .body
        {
            Body::GetCount { count, .. } => count,
            _ => 0,
        };
        let shown = top.is_some_and(|top| visible > 0 && index >= top && index < top + visible);
        if !shown {
            self.list_set_top(list, index);
        }
    }

    /// `XuiList_FocusSelf` 0x92156f40.
    fn list_focus_self(
        &mut self,
        list_element: ElementId,
        focus: ElementId,
    ) -> Result<(), crate::error::FocusError> {
        if let Some(state) = list(self, list_element) {
            state.focus_self_active = true;
        }
        let user = self
            .focus_user(focus)
            .unwrap_or(crate::message::FocusUser::Shared);
        let result = self.set_focus(Some(list_element), user);
        if let Some(state) = list(self, list_element) {
            state.focus_self_active = false;
        }
        result
    }

    /// `XuiList_LayoutItems` 0x92161fd8.
    fn list_layout_items(&mut self, list_element: ElementId) {
        let Some(state) = snapshot(self, list_element) else {
            return;
        };
        for (index, item) in state.items.iter().enumerate() {
            self.list_position_item(list_element, &state, index, *item);
        }
    }

    /// `XuiList_PositionItem` 0x9215dea8.
    fn list_position_item(
        &mut self,
        list_element: ElementId,
        state: &ListState,
        index: usize,
        item: ElementId,
    ) {
        let rect = item_rect(state, index);
        let _ = self.tree.set_property(
            item,
            &[props::position()],
            None,
            PropValue::Vector(Vector3::new(rect.x, rect.y, rect.z)),
        );
        self.tree.set_bounds(item, rect.w, rect.h);
        self.flush_size_changes();

        let (w, h) = size(self, list_element);
        let shown = self.is_shown(item);
        let remaining = items_from_top(state);
        let check = match state.layout {
            ListLayout::VerticalList if state.var_h => Some(rect.y + rect.h > h),
            ListLayout::HorizontalList if state.var_w => Some(rect.x + rect.w > w),
            _ => None,
        };
        match check {
            Some(true) if shown && index != 0 => self.set_show(item, false),
            Some(false) if !shown && (index as u64) < remaining => self.set_show(item, true),
            _ => {}
        }
    }

    /// `XuiList_RefreshItems` 0x92165548.
    fn list_refresh_items(&mut self, list_element: ElementId) {
        let old_count = list(self, list_element).map_or(0, |state| state.count);
        let count = self.list_item_count(list_element);
        let Some(state) = list(self, list_element) else {
            return;
        };
        state.count = count;
        let items = Rc::clone(&state.items);
        let (prev, next, bar) = (state.prev_end, state.next_end, state.bar);

        if count == 0 {
            state.top = None;
            state.cur = None;
            state.slot = None;
            for item in items.iter().copied() {
                self.set_control_index(item, None);
                self.set_show(item, false);
            }
            self.scroll_end_set(prev, ScrollEndState::Normal);
            self.scroll_end_set(next, ScrollEndState::Normal);
            if let Some(bar) = bar {
                self.send_body(
                    bar,
                    Body::ScrollBarSetRange {
                        count: 0,
                        top: Some(0),
                        visible: 0,
                    },
                );
            }
            return;
        }

        if old_count == 0 {
            self.list_set_top(list_element, 0);
            self.list_set_cur_sel(list_element, Some(0));
            if let Some(bar) = bar {
                self.send_body(
                    bar,
                    Body::ScrollBarSetRange {
                        count,
                        top: Some(0),
                        visible: 1,
                    },
                );
            }
            return;
        }

        if state.cur.is_some_and(|cur| cur >= count) {
            state.cur = None;
            state.slot = None;
        }
        let list_size = size(self, list_element);
        let Some(state) = snapshot(self, list_element) else {
            return;
        };
        let max = max_top(&state, list_size);
        if state.top.is_some_and(|top| top > max) {
            self.list_set_top(list_element, max);
        } else {
            let focus = self.focus_owner(list_element);
            let enabled = self.is_enabled(list_element);
            for (slot, item) in state.items.iter().copied().enumerate() {
                let Some(index) = state
                    .top
                    .map(|top| top + slot as u32)
                    .filter(|index| *index < count)
                else {
                    self.set_control_index(item, None);
                    if self.is_shown(item) {
                        self.set_show(item, false);
                    }
                    continue;
                };

                self.set_control_index(item, Some(index));
                if !self.is_shown(item) {
                    self.set_show(item, true);
                    self.update_state(item);
                }
                if state.checkable {
                    let checked = self.list_item_check(list_element, index);
                    self.set_control_value(item, u32::from(checked));
                }
                let item_enabled = enabled && self.list_item_enabled(list_element, index);
                if item_enabled != self.is_enabled(item) {
                    self.send_body(
                        item,
                        Body::SetEnabled {
                            enabled: item_enabled,
                        },
                    );
                }

                if state.cur == Some(index) {
                    let old_slot = list(self, list_element).and_then(|state| state.slot);
                    if let Some(state) = list(self, list_element) {
                        state.slot = Some(slot);
                    }
                    if focus == Some(list_element) {
                        let user = self
                            .focus_user(list_element)
                            .unwrap_or(crate::message::FocusUser::Shared);
                        let _ = self.set_focus(Some(item), user);
                    } else if focus.is_none() {
                        if old_slot != Some(slot) {
                            self.update_state(item);
                        }
                        if let Some(old_slot) = old_slot
                            && let Some(old_item) = state.items.get(old_slot)
                        {
                            self.update_state(*old_item);
                        }
                    }
                }
            }

            if state.var_h || state.var_w {
                self.list_layout_items(list_element);
            }
            let wrap = wrap(self, list_element);
            let Some(state) = snapshot(self, list_element) else {
                return;
            };
            let next_state = next_end_state(&state, wrap, list_size);
            self.scroll_end_set(state.next_end, next_state);
            let prev_state = if state.top.is_some_and(|top| top > 0) || state.spinner_wrap(wrap) {
                ScrollEndState::More
            } else {
                ScrollEndState::Normal
            };
            self.scroll_end_set(state.prev_end, prev_state);
        }

        let Some(state) = snapshot(self, list_element) else {
            return;
        };
        if let Some(bar) = state.bar {
            self.send_body(
                bar,
                Body::ScrollBarSetRange {
                    count,
                    top: state.top,
                    visible: state.items.len() as u32,
                },
            );
        }
    }

    /// `XuiList_ReleaseItems` 0x92157d70.
    fn list_release_items(&mut self, list_element: ElementId) {
        let Some(state) = list(self, list_element) else {
            return;
        };
        let items = core::mem::take(&mut state.items);
        let ends = [
            state.prev_end.take(),
            state.next_end.take(),
            state.bar.take(),
        ];
        state.template = None;

        if let Some(owner) = self.focus_owner(list_element)
            && owner != list_element
        {
            self.unlink(owner);
            let _ = self.list_focus_self(list_element, owner);
        }

        for end in ends.into_iter().flatten() {
            self.destroy(end);
        }
        for item in items.iter().copied() {
            self.destroy(item);
        }
    }

    /// `XuiList_InitFromVisual` 0x921666b8 and `XuiList_CreateItems`.
    fn list_init_from_visual(&mut self, list_element: ElementId, visual: ElementId) -> bool {
        self.list_release_items(list_element);

        let children: Vec<ElementId> = self.tree.children(visual).collect();
        let mut template = None;
        let (mut prev, mut next, mut bar) = (None, None, None);
        for child in children {
            if self.is_builtin_a(child, Builtin::ScrollEnd) {
                let direction =
                    match self
                        .tree
                        .property(child, &[props::scroll_end_direction()], None)
                    {
                        Some(PropValue::Unsigned(direction)) => direction,
                        _ => 0,
                    };
                if matches!(direction, 0 | 2) {
                    prev = prev.or(Some(child));
                } else {
                    next = next.or(Some(child));
                }
            } else if self.is_builtin_a(child, Builtin::ScrollBar) {
                bar = bar.or(Some(child));
            } else if self.is_builtin_a(child, Builtin::ListItem) && template.is_none() {
                template = Some(child);
            }
        }

        let Some(template) = template else {
            return false;
        };

        let (position, item_size, anchor) = match self.tree.get(template) {
            Some(element) => (element.position(), element.size(), element.anchor()),
            None => return false,
        };
        let layout = match self
            .tree
            .property(template, &[props::list_item_layout()], None)
        {
            // The Layout setter rejects values above 5, leaving the default.
            Some(PropValue::Unsigned(layout)) => ListLayout::from_raw(layout).unwrap_or_default(),
            _ => ListLayout::default(),
        };
        let flag = |cx: &Self, def| {
            matches!(
                cx.tree.property(template, &[def], None),
                Some(PropValue::Bool(true))
            )
        };
        let checkable = flag(self, props::list_item_checkable());
        let keep = flag(self, props::list_item_keep_size_unfocused());
        let vector = |cx: &Self, def| match cx.tree.property(template, &[def], None) {
            Some(PropValue::Vector(value)) => value,
            _ => Vector3::new(0.0, 0.0, 0.0),
        };
        let selected = vector(self, props::list_item_selected_size());
        let spacing = vector(self, props::list_item_spacing());

        let Some(state) = list(self, list_element) else {
            return false;
        };
        state.template_position = Vector3Copy {
            x: position.x,
            y: position.y,
            z: position.z,
        };
        state.item_cur_w = item_size.width;
        state.item_cur_h = item_size.height;
        state.template_anchor = anchor;
        state.layout = layout;
        state.checkable = checkable;
        state.keep_size_unfocused = keep;
        state.sel_h = state.item_h;
        state.sel_w = state.item_w;
        state.sel_cur_h = state.item_cur_h;
        state.sel_cur_w = state.item_cur_w;
        if selected.y > state.item_h {
            state.sel_h = selected.y;
        }
        if selected.x > state.item_w {
            state.sel_w = selected.x;
        }
        state.spacing_y = spacing.y.max(0.0);
        state.spacing_x = spacing.x.max(0.0);

        // `XuiList_AdjustTemplateAnchor`.
        state.multi = true;
        match layout {
            ListLayout::VerticalList => state.template_anchor.clear(Anchor::TOP | Anchor::BOTTOM),
            ListLayout::HorizontalList => state.template_anchor.clear(Anchor::LEFT | Anchor::RIGHT),
            ListLayout::GridByRow | ListLayout::GridByColumn => state
                .template_anchor
                .clear(Anchor::LEFT | Anchor::TOP | Anchor::RIGHT | Anchor::BOTTOM),
            ListLayout::VerticalSpinner | ListLayout::HorizontalSpinner => state.multi = false,
        }

        self.runtime.load_depth += 1;
        for part in [prev, next, bar, Some(template)].into_iter().flatten() {
            let _ = self.tree.unlink(part);
        }
        self.runtime.load_depth -= 1;
        self.destroy(template);

        let Some(state) = list(self, list_element) else {
            return false;
        };
        state.prev_end = prev;
        state.next_end = next;
        state.bar = bar;

        // The template record is the first ListItem child of the visual
        // definition, found again by the visual's Id.
        let visual_id: Option<Vec<u16>> = self
            .tree
            .get(visual)
            .and_then(|visual| visual.id())
            .map(Into::into);
        let prefix = self.skin_prefix(list_element);
        let record = visual_id.and_then(|id| self.runtime.skins.lookup(&id, prefix.as_deref()));
        let template_record = record.and_then(|record| {
            self.runtime
                .skins
                .find_child_record(self.registry, record, |class| {
                    self.registry
                        .find("XuiListItem")
                        .is_some_and(|item| class.is_a(item))
                })
        });
        let Some(template_record) = template_record else {
            return false;
        };
        if let Some(state) = list(self, list_element) {
            state.template = Some(template_record);
        }

        self.list_create_items(list_element)
    }

    fn list_create_items(&mut self, list_element: ElementId) -> bool {
        let Some(template) = list(self, list_element).and_then(|state| state.template) else {
            return false;
        };
        let Ok(clone) = self.instantiate_skin_object(template, true) else {
            return false;
        };
        let clone_size = self
            .tree
            .get(clone)
            .map(|clone| clone.size())
            .unwrap_or_default();
        self.destroy(clone);

        let (list_w, list_h) = size(self, list_element);
        let Some(state) = list(self, list_element) else {
            return false;
        };
        state.item_w = clone_size.width.max(1.0);
        state.item_h = clone_size.height.max(1.0);
        let (w, h) = (state.item_w, state.item_h);
        state.sel_h = state.sel_h.max(h);
        state.sel_w = state.sel_w.max(w);
        let variable = matches!(
            state.layout,
            ListLayout::VerticalList | ListLayout::HorizontalList
        );
        state.var_h = variable && (state.sel_h - h).abs() >= 1.192_092_9e-7;
        state.var_w = variable && (state.sel_w - w).abs() >= 1.192_092_9e-7;
        if !state.var_h {
            state.sel_h = h;
        }
        if !state.var_w {
            state.sel_w = w;
        }

        let ctrls = recount(state, list_w, list_h);

        if let Some(slot) = state.slot
            && slot >= ctrls
        {
            if let Some(cur) = state.cur {
                // Odd adjustment kept from the binary.
                state.cur = (slot as i64 - ctrls as i64 + i64::from(cur) - 1)
                    .try_into()
                    .ok();
            }
            // Not 1:1: the original keeps the slot, which then indexes past
            // the new item array until RefreshItems or a selection sets it.
            state.slot = None;
        }

        let anchor = state.template_anchor;
        let mut items = Vec::new();
        for index in 0..ctrls {
            let Ok(item) = self.instantiate_skin_object(template, true) else {
                return false;
            };
            let _ = self.tree.set_property(
                item,
                &[props::anchor()],
                None,
                PropValue::Unsigned(anchor.0),
            );
            let Some(state) = snapshot(self, list_element) else {
                return false;
            };
            self.list_position_item(list_element, &state, index, item);
            let _ = self.tree.set_show(item, false);
            let _ = self.insert_child(list_element, item, None);
            items.push(item);
            if let Some(state) = list(self, list_element) {
                state.items = Rc::from(items.as_slice());
            }
        }

        self.runtime.load_depth += 1;
        let parts =
            list(self, list_element).map(|state| [state.prev_end, state.next_end, state.bar]);
        for part in parts.into_iter().flatten().flatten() {
            let _ = self.tree.insert_child(list_element, part, None);
        }
        self.runtime.load_depth -= 1;

        self.list_refresh_items(list_element);
        true
    }
}

/// Fits items into the list size; returns the item control count.
fn recount(state: &mut ListState, list_w: f32, list_h: f32) -> usize {
    fn fit(available: f32, size: f32, spacing: f32) -> i32 {
        let pitch = size + spacing;
        let quotient = available / pitch;
        let fraction = quotient - libm::truncf(quotient);
        if fraction * pitch >= size {
            libm::ceilf(quotient) as i32
        } else {
            libm::floorf(quotient) as i32
        }
    }

    let (w, h) = (state.item_w, state.item_h);
    let p = state.template_position;
    let mut ctrls: i32;
    if !state.multi {
        state.rows = 1;
        state.cols = 1;
        ctrls = 1;
    } else {
        match state.layout {
            ListLayout::VerticalList => {
                let n = fit(list_h - p.y, h, state.spacing_y);
                ctrls = n;
                state.rows = n.max(0) as u32;
                state.cols = 1;
                state.sel_cur_h = state.sel_h;
                state.sel_cur_w = libm::floorf(state.item_cur_w / w * state.sel_w + 0.5);
            }
            ListLayout::HorizontalList => {
                let n = fit(list_w - p.x, w, state.spacing_x);
                ctrls = n;
                state.cols = n.max(0) as u32;
                state.rows = 1;
                state.sel_cur_w = state.sel_w;
                state.sel_cur_h = libm::floorf(state.item_cur_h / h * state.sel_h + 0.5);
            }
            ListLayout::VerticalSpinner
            | ListLayout::HorizontalSpinner
            | ListLayout::GridByRow
            | ListLayout::GridByColumn => {
                let rows = fit(list_h - p.y, h, state.spacing_y);
                let cols = fit(list_w - p.x, w, state.spacing_x);
                state.rows = rows.max(0) as u32;
                state.cols = cols.max(0) as u32;
                // 32-bit multiply that wraps, as in the binary.
                ctrls = rows.wrapping_mul(cols);
            }
        }
    }

    state.rows = state.rows.max(1);
    state.cols = state.cols.max(1);
    if ctrls < 1 {
        ctrls = match state.layout {
            ListLayout::GridByRow => state.cols as i32,
            ListLayout::GridByColumn => state.rows as i32,
            _ => 1,
        };
    }

    ctrls as usize
}

pub(super) fn on_list(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    class: ClassId,
    message: &mut Message,
) {
    match &mut message.body {
        Body::CreateVisual => {
            message.handled = true;
            if cx.runtime.load_depth > 0 {
                return;
            }
            let mut forwarded = Message::new(Body::CreateVisual);
            cx.send_to_base(element, class, &mut forwarded);
            let Some(visual) = cx.control_visual(element) else {
                cx.list_release_items(element);
                return;
            };
            if !cx.list_init_from_visual(element, visual) {
                cx.list_release_items(element);
            }
            cx.update_state(element);
        }
        Body::Destroy => cx.list_release_items(element),
        Body::SetFocus { previous, user } => {
            let (previous, user) = (*previous, *user);
            message.handled = true;
            cx.notify_parent(element, NotifyKind::SetFocus { previous });
            let Some(state) = snapshot(cx, element) else {
                return;
            };
            if let Some(item) = state.slot.and_then(|slot| state.items.get(slot).copied())
                && !state.focus_self_active
            {
                let _ = cx.set_focus(Some(item), user);
                let wrap = wrap(cx, element);
                let list_size = size(cx, element);
                let next = if is_at_end(&state, list_size) {
                    if state.spinner_wrap(wrap) {
                        ScrollEndState::More
                    } else {
                        ScrollEndState::Normal
                    }
                } else {
                    ScrollEndState::More
                };
                cx.scroll_end_set(state.next_end, next);
            }
            if let Some(state) = list(cx, element) {
                state.focus_self_active = false;
            }
            if previous.is_none_or(|previous| !cx.is_descendant(previous, element)) {
                cx.update_state_focus(element, true);
            }
            if let Some(state) = list(cx, element) {
                state.focus_within = true;
            }
        }
        Body::KillFocus { new, .. } => {
            let new = *new;
            if new.is_some_and(|new| cx.is_descendant(new, element)) {
                message.handled = true;
                cx.notify_parent(element, NotifyKind::KillFocus { new });
            }
            if let Some(state) = list(cx, element) {
                state.focus_within = false;
            }
        }
        Body::Notify(notify) => {
            let source = notify.source;
            let kind = notify.kind.clone();
            on_list_notify(cx, element, source, kind, message);
        }
        Body::SizeChanging { old, new } => {
            let (old, new) = (*old, *new);
            message.handled = true;
            list_size_changed(cx, element, old, new);
        }
        Body::KeyDown(key) => {
            let key = *key;
            let keyboard = !cx.runtime.keyboard_keys_disabled;
            match key_to_nav(&key, keyboard) {
                Some(NavKey::PageUp) => {
                    message.handled = true;
                    page(cx, element, false);
                }
                Some(NavKey::PageDown) => {
                    message.handled = true;
                    page(cx, element, true);
                }
                _ => {}
            }
        }
        Body::SetEnabled { enabled } => {
            let enabled = *enabled;
            let Some(state) = snapshot(cx, element) else {
                return;
            };
            for (slot, item) in state.items.iter().copied().enumerate() {
                let Some(index) = state
                    .top
                    .map(|top| top + slot as u32)
                    .filter(|index| *index < state.count)
                else {
                    continue;
                };
                let item_enabled = enabled && cx.list_item_enabled(element, index);
                if cx.is_enabled(item) != item_enabled {
                    cx.send_body(
                        item,
                        Body::SetEnabled {
                            enabled: item_enabled,
                        },
                    );
                }
            }
        }
        Body::GetCurItem { .. } => {
            let Some(state) = snapshot(cx, element) else {
                return;
            };
            let item = state.slot.and_then(|slot| state.items.get(slot).copied());
            message.body = Body::GetCurItem {
                index: state.cur,
                item,
            };
            message.handled = true;
        }
        Body::SetCurItem { index } => {
            let wanted = *index;
            message.handled = true;
            let index = set_cur_sel(cx, element, wanted);
            message.body = Body::SetCurItem { index };
        }
        Body::GetTop { top } => {
            *top = None;
            let current = list(cx, element).and_then(|state| state.top);
            message.body = Body::GetTop { top: current };
            message.handled = true;
        }
        Body::SetTop { top } => {
            let wanted = *top;
            message.handled = true;
            let top = set_top(cx, element, wanted);
            message.body = Body::SetTop { top };
        }
        Body::GetCount { kind, count } => {
            let kind = *kind;
            let Some(state) = snapshot(cx, element) else {
                return;
            };
            let list_size = size(cx, element);
            let answer = match kind {
                CountKind::Visible => Some(visible_count(&state, list_size)),
                CountKind::ItemControls => Some(state.items.len() as u32),
                CountKind::Lines => Some(if state.layout.is_horizontal() {
                    state.cols
                } else {
                    state.rows
                }),
                CountKind::PerLine => Some(state.per_line()),
                CountKind::Items | CountKind::Checked => None,
            };
            if let Some(answer) = answer {
                *count = answer;
                message.handled = true;
            }
        }
        Body::InsertItems { .. } | Body::DeleteItems { .. } => {
            message.handled = true;
            if cx.control_visual(element).is_none() {
                cx.send_body(element, Body::CreateResources);
            }
            cx.list_refresh_items(element);
        }
        Body::SetItemCheck { index, checked } => {
            let (index, checked) = (*index, *checked);
            message.handled = true;
            let Some(state) = snapshot(cx, element) else {
                return;
            };
            if !state.checkable || index >= state.count {
                return;
            }
            let list_size = size(cx, element);
            let visible = visible_count(&state, list_size);
            if let Some(item) = slot_for(Some(index), state.top, visible)
                .and_then(|slot| state.items.get(slot).copied())
                && (cx.control_value(item) != 0) != checked
            {
                cx.set_control_value(item, u32::from(checked));
            }
            cx.bubble_notify(
                Some(element),
                element,
                NotifyKind::ItemCheckChanged { index, checked },
            );
        }
        Body::GetNavTarget(query) => {
            let query = *query;
            list_navigate(cx, element, query, message);
        }
        Body::GetItemEnabled { enabled, .. } => {
            *enabled = true;
            message.handled = true;
        }
        Body::SetItemEnabled { index, enabled } => {
            let (index, enabled) = (*index, *enabled);
            message.handled = true;
            let Some(state) = snapshot(cx, element) else {
                return;
            };
            let list_size = size(cx, element);
            let visible = visible_count(&state, list_size);
            if state.count != 0
                && let Some(item) = slot_for(Some(index), state.top, visible)
                    .and_then(|slot| state.items.get(slot).copied())
                && cx.is_enabled(item) != enabled
            {
                cx.send_body(item, Body::SetEnabled { enabled });
            }
        }
        Body::FindTabStop(query) => {
            if matches!(query.direction, TabStopKind::First | TabStopKind::Last) {
                let (cur, _) = cx.cur_item(element);
                if let Some(cur) = cur {
                    cx.list_set_cur_sel_visible(element, cur);
                    if let Body::FindTabStop(query) = &mut message.body {
                        query.target_is_source = true;
                    }
                    message.handled = true;
                }
            }
        }
        _ => {}
    }
}

fn on_list_notify(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    source: ElementId,
    kind: NotifyKind,
    message: &mut Message,
) {
    match kind {
        NotifyKind::Press { user } => {
            message.handled = true;
            let checkable = list(cx, element).is_some_and(|state| state.checkable);
            if checkable && let Some(index) = cx.control_index(source) {
                let list_check = cx.list_item_check(element, index);
                let item_check = cx.control_value(source) != 0;
                if list_check != item_check {
                    cx.send_body(
                        element,
                        Body::SetItemCheck {
                            index,
                            checked: item_check,
                        },
                    );
                }
            }
            cx.notify_press(element, user);
        }
        NotifyKind::SetFocus { previous } => {
            if cx.is_descendant(source, element)
                && previous.is_none_or(|previous| !cx.is_descendant(previous, element))
            {
                cx.update_state_focus(element, true);
            }
            if let Some(state) = list(cx, element) {
                state.focus_within = true;
                if state.var_h || state.var_w {
                    cx.list_layout_items(element);
                }
            }
        }
        NotifyKind::KillFocus { new } => {
            let inside = new.is_some_and(|new| cx.is_descendant(new, element));
            if !inside {
                cx.update_state_focus(element, false);
            }
            let Some(state) = list(cx, element) else {
                return;
            };
            state.focus_within = false;
            let variable = state.var_h || state.var_w;
            if variable {
                cx.list_layout_items(element);
            }
            if !inside && let Some(state) = snapshot(cx, element) {
                let wrap = wrap(cx, element);
                let list_size = size(cx, element);
                let next = if is_at_end(&state, list_size) {
                    if state.spinner_wrap(wrap) {
                        ScrollEndState::More
                    } else {
                        ScrollEndState::Normal
                    }
                } else {
                    ScrollEndState::More
                };
                cx.scroll_end_set(state.next_end, next);
            }
        }
        NotifyKind::ScrollEndStateRequest => {
            message.handled = true;
            let Some(state) = snapshot(cx, element) else {
                return;
            };
            let wrap = wrap(cx, element);
            let list_size = size(cx, element);
            if Some(source) == state.prev_end {
                let more = state.top.is_some_and(|top| top > 0) || state.spinner_wrap(wrap);
                let value = if more {
                    ScrollEndState::More
                } else {
                    ScrollEndState::Normal
                };
                cx.scroll_end_set(state.prev_end, value);
            }
            if Some(source) == state.next_end {
                cx.scroll_end_set(state.next_end, next_end_state(&state, wrap, list_size));
            }
        }
        NotifyKind::Pressing { repeat, user, .. } => {
            message.handled = true;
            let cancel = cx.notify_pressing(element, user, repeat);
            if let Body::Notify(notify) = &mut message.body
                && let NotifyKind::Pressing { cancel: out, .. } = &mut notify.kind
            {
                *out = cancel;
            }
        }
        NotifyKind::ScrollBarScroll { top } => {
            message.handled = true;
            if list(cx, element).and_then(|state| state.bar) == Some(source) {
                cx.list_set_top(element, top);
                if !cx.has_focus(element) {
                    let _ = cx.set_focus(Some(element), crate::message::FocusUser::Shared);
                }
            }
        }
        _ => {}
    }
}

/// 0x7d8 at the List level. Returns the clamped index written back.
fn set_cur_sel(cx: &mut Context<'_, '_>, element: ElementId, wanted: Option<u32>) -> Option<u32> {
    let Some(state) = snapshot(cx, element) else {
        return wanted;
    };
    let index = match wanted {
        None => None,
        Some(index) if index >= state.count => state.count.checked_sub(1),
        Some(index) => Some(index),
    };

    let old = state.cur;
    if index == old {
        return index;
    }

    let old_slot = state.slot;
    let list_size = size(cx, element);
    let Some(state) = list(cx, element) else {
        return index;
    };
    state.cur = index;
    state.slot = slot_for(index, state.top, visible_count(state, list_size));
    let snapshot_state = state.clone();
    let item = snapshot_state
        .slot
        .and_then(|slot| snapshot_state.items.get(slot).copied());

    if !snapshot_state.items.is_empty() {
        if snapshot_state.var_h || snapshot_state.var_w {
            cx.list_layout_items(element);
        }

        let focus = cx.focus_owner(element);
        match focus {
            Some(focus) => {
                let user = cx
                    .focus_user(focus)
                    .unwrap_or(crate::message::FocusUser::Shared);
                if let Some(item) = item {
                    let _ = cx.set_focus(Some(item), user);
                } else if focus != element {
                    let result = cx.list_focus_self(element, focus);
                    if result.is_err()
                        && let Some(state) = list(cx, element)
                    {
                        state.slot = old_slot;
                        state.cur = old;
                    }
                }
            }
            None => {
                if let Some(slot) = old_slot
                    && let Some(item) = snapshot_state.items.get(slot)
                {
                    cx.update_state(*item);
                }
                if let Some(item) = item {
                    cx.update_state(item);
                }
            }
        }
    }

    // Sent even when focus could not move and the index was reverted.
    let cur = list(cx, element).and_then(|state| state.cur);
    cx.bubble_notify(
        Some(element),
        element,
        NotifyKind::SelectionChanged { new: cur, old },
    );

    index
}

/// 0x7db at the List level. Returns the value written back.
fn set_top(cx: &mut Context<'_, '_>, element: ElementId, wanted: Option<u32>) -> Option<u32> {
    // A negative top is rejected (E_INVALIDARG) while still handled.
    let wanted = wanted?;
    let Some(state) = snapshot(cx, element) else {
        return Some(wanted);
    };
    if state.count == 0 {
        return None;
    }
    if Some(wanted) == state.top {
        return Some(wanted);
    }

    let list_size = size(cx, element);
    let mut top = wanted.min(max_top(&state, list_size));
    let per_line = state.per_line();
    if per_line != 0 {
        top -= top % per_line;
    }
    if Some(top) == state.top {
        return Some(top);
    }

    let wrap = wrap(cx, element);
    let old_top = state.top;
    let forward = if state.spinner_wrap(wrap) {
        if old_top == Some(state.count - 1) && top == 0 {
            true
        } else if old_top == Some(0) && top == state.count - 1 {
            false
        } else {
            old_top.is_none_or(|old| old < top)
        }
    } else {
        old_top.is_none_or(|old| old < top)
    };

    let Some(state) = list(cx, element) else {
        return Some(top);
    };
    state.top = Some(top);
    state.slot = slot_for(state.cur, state.top, visible_count(state, list_size));
    let state = state.clone();

    let enabled = cx.is_enabled(element);
    for (slot, item) in state.items.iter().copied().enumerate() {
        let index = top + slot as u32;
        if index >= state.count {
            cx.set_control_index(item, None);
            cx.set_show(item, false);
            continue;
        }

        cx.set_control_index(item, Some(index));
        cx.set_show(item, true);
        if state.checkable {
            let checked = cx.list_item_check(element, index);
            cx.set_control_value(item, u32::from(checked));
        }
        let item_enabled = enabled && cx.list_item_enabled(element, index);
        cx.send_body(
            item,
            Body::SetEnabled {
                enabled: item_enabled,
            },
        );
    }

    let focus = cx.focus_owner(element);
    match state.slot.and_then(|slot| state.items.get(slot).copied()) {
        None => {
            if let Some(focus) = focus
                && focus != element
            {
                let _ = cx.list_focus_self(element, focus);
            }
        }
        Some(item) => {
            if let Some(focus) = focus
                && focus != item
            {
                let user = cx
                    .focus_user(focus)
                    .unwrap_or(crate::message::FocusUser::Shared);
                let _ = cx.set_focus(Some(item), user);
            }
        }
    }

    if state.var_h || state.var_w {
        cx.list_layout_items(element);
    }

    let Some(state) = snapshot(cx, element) else {
        return Some(top);
    };
    let next = if state.spinner_wrap(wrap) || !is_at_end(&state, list_size) {
        if forward {
            ScrollEndState::Scrolling
        } else {
            ScrollEndState::More
        }
    } else {
        ScrollEndState::Normal
    };
    cx.scroll_end_set(state.next_end, next);
    let prev = if state.spinner_wrap(wrap) || top > 0 {
        if forward {
            ScrollEndState::More
        } else {
            ScrollEndState::Scrolling
        }
    } else {
        ScrollEndState::Normal
    };
    cx.scroll_end_set(state.prev_end, prev);
    if let Some(bar) = state.bar {
        cx.send_body(
            bar,
            Body::ScrollBarSetRange {
                count: state.count,
                top: Some(top),
                visible: state.items.len() as u32,
            },
        );
    }

    Some(top)
}

/// 0x7e6 at the List level: moves between items.
fn list_navigate(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    query: NavQuery<NavDirection>,
    message: &mut Message,
) {
    if matches!(
        query.direction,
        NavDirection::TabForward | NavDirection::TabBackward
    ) {
        // Tab moves relative to the list.
        if let Body::GetNavTarget(query) = &mut message.body {
            query.source = element;
        }
        return;
    }
    if query.source == element {
        return;
    }

    let Some(state) = snapshot(cx, element) else {
        return;
    };
    let horizontal = state.layout.is_horizontal();
    let (prev_dir, next_dir) = if horizontal {
        (NavDirection::Left, NavDirection::Right)
    } else {
        (NavDirection::Up, NavDirection::Down)
    };
    let cross = match state.layout {
        ListLayout::GridByRow => Some((NavDirection::Left, NavDirection::Right)),
        ListLayout::GridByColumn => Some((NavDirection::Up, NavDirection::Down)),
        _ => None,
    };
    let Some(slot) = state.items.iter().position(|item| *item == query.source) else {
        return;
    };

    // Rows and cols are at least 1 once item controls exist.
    let per_line = state.per_line();
    if per_line == 0 {
        return;
    }
    let slot_index = slot as u32;
    let count = state.count;
    let cur = state.cur;
    let top0 = state.top;
    let wrap = wrap(cx, element);
    let list_size = size(cx, element);

    let mut changed = false;
    let direction = query.direction;
    if direction == prev_dir {
        // cur - perLine; a negative index selects no item.
        let target = cur.and_then(|cur| cur.checked_sub(per_line));
        if slot_index >= per_line {
            cx.list_set_cur_sel(element, target);
        } else if let Some(top) = top0.filter(|top| *top > 0) {
            cx.list_set_cur_sel(element, target);
            let (after, top_now) =
                list(cx, element).map_or((None, None), |state| (state.cur, state.top));
            // The original compares with cur - perLine and stores no item as
            // -1, so clearing the selection from perLine - 1 also matches.
            let reached = after == target && (target.is_some() || cur == Some(per_line - 1));
            // 0x7db rejects a negative top, so the list stays put.
            if reached
                && top_now == top0
                && let Some(new_top) = top.checked_sub(per_line)
            {
                cx.list_set_top(element, new_top);
            }
            changed = true;
        } else if top0 == Some(0) && wrap {
            let remainder = count % per_line;
            let next = match cur {
                Some(cur) if remainder == 0 => cur
                    .checked_add(count)
                    .and_then(|sum| sum.checked_sub(per_line)),
                Some(cur) if cur >= remainder => count.checked_sub(1),
                Some(cur) => cur
                    .checked_add(count)
                    .and_then(|sum| sum.checked_sub(remainder)),
                // The original computes these from cur = -1.
                None if remainder == 0 => count.checked_sub(per_line + 1),
                None => count.checked_sub(remainder + 1),
            };
            select_visible(cx, element, next);
            changed = true;
        }
    } else if direction == next_dir {
        // min(cur + perLine, count - 1), with no item as -1.
        let next = count.checked_sub(1).map(|last| {
            cur.map_or(per_line - 1, |cur| cur.saturating_add(per_line))
                .min(last)
        });
        let last_line = count.checked_sub(if count % per_line != 0 {
            count % per_line
        } else {
            per_line
        });
        let visible = visible_count(&state, list_size).max(1);
        let below_limit = last_line.is_some_and(|last_line| {
            slot_index < last_line.min((visible - 1) / per_line * per_line)
        });
        // Start of the line after cur's. The original truncates -1 % perLine
        // to -1, or to 0 with one item per line.
        let line_end = match cur {
            Some(cur) => u64::from(cur - cur % per_line) + u64::from(per_line),
            None if per_line == 1 => 0,
            None => u64::from(per_line),
        };
        if below_limit {
            cx.list_set_cur_sel(element, next);
        } else if line_end < u64::from(count) {
            cx.list_set_cur_sel(element, next);
            let (after, top_now) =
                list(cx, element).map_or((None, None), |state| (state.cur, state.top));
            // The top is set whenever count is nonzero.
            if after == next
                && top_now == top0
                && let Some(top) = top0
            {
                cx.list_set_top(element, top.saturating_add(per_line));
            }
            changed = true;
        } else if wrap {
            // cur % perLine, with no item as -1.
            let target = match cur {
                Some(cur) => Some(cur % per_line),
                None if per_line == 1 => Some(0),
                None => None,
            };
            select_visible(cx, element, target);
            changed = true;
        }
    } else if let Some((cross_prev, cross_next)) = cross {
        if direction == cross_prev {
            if !slot_index.is_multiple_of(per_line) {
                cx.list_set_cur_sel(element, cur.and_then(|cur| cur.checked_sub(1)));
            }
        } else if direction == cross_next && slot_index % per_line < per_line - 1 {
            // cur + 1 when cur < count; no item (-1) moves to item 0.
            let target = match cur {
                Some(cur) => (cur < count).then(|| cur + 1),
                None => Some(0),
            };
            if let Some(target) = target {
                cx.list_set_cur_sel(element, Some(target));
            }
        }
    }

    let slot_now = list(cx, element).and_then(|state| state.slot);
    if slot_now != Some(slot) || changed {
        message.handled = true;
        if let Body::GetNavTarget(query) = &mut message.body {
            query.target_is_source = true;
        }
    }
}

/// `XuiListSetCurSelVisible` for an index the original computes as negative
/// when there is none: the selection clears and 0x7db rejects the top.
fn select_visible(cx: &mut Context<'_, '_>, element: ElementId, index: Option<u32>) {
    match index {
        Some(index) => cx.list_set_cur_sel_visible(element, index),
        None => cx.list_set_cur_sel(element, None),
    }
}

/// Page keys at the List level.
fn page(cx: &mut Context<'_, '_>, element: ElementId, down: bool) {
    let Some(state) = snapshot(cx, element) else {
        return;
    };
    let Some(top) = state.top else {
        return;
    };

    let list_size = size(cx, element);
    let visible = visible_count(&state, list_size);
    // A valid slot implies a current item.
    let valid_cur = state.cur.filter(|_| cur_slot_valid(&state, visible));
    let valid = valid_cur.is_some();
    let ctrls = state.items.len() as u32;
    let count = state.count;
    let on_first = valid && state.slot == Some(0);
    let on_last = valid && state.slot.is_some_and(|slot| slot as u32 + 1 == visible);

    if !down {
        if count > ctrls && top != 0 {
            if !on_first && valid {
                cx.list_set_cur_sel(element, Some(top));
            } else {
                let new_top = top.saturating_sub(visible);
                if let Some(cur) = valid_cur {
                    let target =
                        (i64::from(cur) - i64::from(top) + i64::from(new_top)).max(0) as u32;
                    cx.list_set_cur_sel(element, Some(target));
                }
                cx.list_set_top(element, new_top);
            }
        } else if !on_first && valid {
            cx.list_set_cur_sel(element, Some(0));
        }
        return;
    }

    if count > ctrls && visible + top < count {
        if !on_last && valid {
            cx.list_set_cur_sel(element, Some(visible + top - 1));
        } else {
            let mut new_top = top + visible;
            if visible + new_top >= count {
                new_top = count - visible;
            }
            let per_line = state.per_line();
            if per_line != 0 && !new_top.is_multiple_of(per_line) {
                new_top += per_line - new_top % per_line;
            }
            if let Some(cur) = valid_cur {
                let target = (i64::from(cur) - i64::from(top) + i64::from(new_top))
                    .min(i64::from(count) - 1);
                cx.list_set_cur_sel(element, u32::try_from(target).ok());
            }
            cx.list_set_top(element, new_top);
        }
    } else if !on_last && valid {
        cx.list_set_cur_sel(element, count.checked_sub(1));
    }
}

/// `XuiList_OnSizeChanged` 0x92165968.
fn list_size_changed(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    old: (f32, f32),
    new: (f32, f32),
) {
    let Some(state) = snapshot(cx, element) else {
        return;
    };
    if state.items.is_empty() {
        return;
    }

    if state.layout.is_grid() {
        cx.list_layout_items(element);
    }

    let first = state.items[0];
    let first_size = cx
        .tree
        .get(first)
        .map(|item| item.size())
        .unwrap_or_default();
    let (anchor, scale) = cx
        .tree
        .get(first)
        .map_or((Anchor::default(), Vector3::new(1.0, 1.0, 1.0)), |item| {
            (item.anchor(), item.scale())
        });
    let Some(list_state) = list(cx, element) else {
        return;
    };
    if !list_state.var_h && !list_state.var_w {
        list_state.item_cur_w = first_size.width;
        list_state.item_cur_h = first_size.height;
    } else {
        if anchor.contains(Anchor::LEFT | Anchor::RIGHT) {
            list_state.item_cur_w += (new.0 - old.0) / scale.x;
        }
        if anchor.contains(Anchor::TOP | Anchor::BOTTOM) {
            list_state.item_cur_h += (new.1 - old.1) / scale.y;
        }
    }
    if !list_state.multi {
        return;
    }

    let ctrls = list_state.items.len();
    let n = recount(list_state, new.0, new.1);
    if n < ctrls {
        let stale_slot = list_state.slot.is_some_and(|slot| slot >= n);
        if stale_slot {
            if let (Some(slot), Some(cur)) = (list_state.slot, list_state.cur) {
                list_state.cur = (slot as i64 - n as i64 + i64::from(cur) - 1)
                    .try_into()
                    .ok();
            }
            if let Some(focus) = cx.focus_owner(element)
                && focus != element
            {
                let _ = cx.list_focus_self(element, focus);
            }
        }

        let Some(list_state) = list(cx, element) else {
            return;
        };
        let keep = n.min(list_state.items.len());
        let removed = list_state.items[keep..].to_vec();
        list_state.items = Rc::from(&list_state.items[..keep]);
        if stale_slot {
            list_state.slot = n.checked_sub(1);
        }
        for item in removed {
            cx.destroy(item);
        }
        cx.list_layout_items(element);
        cx.list_refresh_items(element);
    } else if n > ctrls {
        let Some(template) = list_state.template else {
            return;
        };
        let anchor = list_state.template_anchor;
        let mut items = list_state.items.to_vec();
        for _ in ctrls..n {
            let Ok(item) = cx.instantiate_skin_object(template, true) else {
                return;
            };
            let _ = cx.tree.set_property(
                item,
                &[props::anchor()],
                None,
                PropValue::Unsigned(anchor.0),
            );
            let _ = cx.tree.set_show(item, false);
            let _ = cx.insert_child(element, item, None);
            items.push(item);
            if let Some(state) = list(cx, element) {
                state.items = Rc::from(items.as_slice());
            }
        }
        cx.list_layout_items(element);

        cx.runtime.load_depth += 1;
        let parts = list(cx, element).map(|state| [state.prev_end, state.next_end, state.bar]);
        for part in parts.into_iter().flatten().flatten() {
            let _ = cx.tree.unlink(part);
            let _ = cx.tree.insert_child(element, part, None);
        }
        cx.runtime.load_depth -= 1;
        cx.list_refresh_items(element);
    } else if list_state.var_h || list_state.var_w {
        cx.list_layout_items(element);
    }
}

fn common<'c>(cx: &'c mut Context<'_, '_>, element: ElementId) -> Option<&'c mut CommonListState> {
    match cx.class_state(element)? {
        ClassState::List { common, .. } => common.as_deref_mut(),
        _ => None,
    }
}

/// `XuiSplitCrLf`: segments ending in CR LF; trailing text is dropped.
fn split_crlf(text: Option<&[u16]>) -> Vec<Rc<[u16]>> {
    let mut parts = Vec::new();
    let Some(text) = text else {
        return parts;
    };

    let mut start = 0;
    let mut index = 0;
    while index + 1 < text.len() {
        if text[index] == 0x0d && text[index + 1] == 0x0a {
            parts.push(Rc::from(&text[start..index]));
            index += 2;
            start = index;
        } else {
            index += 1;
        }
    }

    parts
}

impl Context<'_, '_> {
    /// `XuiCommonList_ApplyItemProps` 0x9216ac80.
    pub fn common_list_apply_item_props(&mut self, element: ElementId) {
        let texts = split_crlf(self.tree.string(element, &[props::items_text()]));
        let images = split_crlf(self.tree.string(element, &[props::items_image()]));
        let paths = split_crlf(self.tree.string(element, &[props::items_nav_path()]));
        let n = texts.len().max(images.len()).max(paths.len()) as u32;

        let current = self.list_item_count(element);
        if n <= current {
            self.send_body(
                element,
                Body::DeleteItems {
                    index: n,
                    count: current - n,
                },
            );
        } else {
            self.send_body(
                element,
                Body::InsertItems {
                    index: current,
                    count: n - current,
                },
            );
        }
        self.list_set_cur_sel(element, Some(0));
        if self.cur_item(element).0 == Some(0) {
            self.list_set_top(element, 0);
        }

        for k in 0..n as usize {
            self.send_body(
                element,
                Body::SetSourceText {
                    item: k as u32,
                    association: 0,
                    item_data: true,
                    text: texts.get(k).cloned(),
                },
            );
        }
        for k in 0..n as usize {
            self.send_body(
                element,
                Body::SetSourceImage {
                    item: k as u32,
                    association: 0,
                    item_data: true,
                    path: images.get(k).cloned(),
                },
            );
        }
        for k in 0..n as usize {
            self.send_body(
                element,
                Body::SetNavData {
                    item: Some(k as u32),
                    data: NavData {
                        path: paths.get(k).cloned(),
                        stay_visible: false,
                        src_trans_index: 0,
                        dest_trans_index: 0,
                    },
                },
            );
        }
    }
}

pub(super) fn on_common_list(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &mut message.body {
        Body::GetCount { kind, count } => match kind {
            CountKind::Items => {
                *count = common(cx, element).map_or(0, |store| store.texts.len() as u32);
                message.handled = true;
            }
            CountKind::Checked => {
                let checked = common(cx, element).map_or(0, |store| {
                    store
                        .flags
                        .iter()
                        .filter(|flags| flags.contains(ItemFlags::CHECKED))
                        .count() as u32
                });
                message.body = Body::GetCount {
                    kind: CountKind::Checked,
                    count: checked,
                };
                message.handled = true;
            }
            _ => {}
        },
        Body::InsertItems { index, count } => {
            let (index, count) = (*index as usize, *count as usize);
            if let Some(store) = common(cx, element)
                && count > 0
                && index <= store.texts.len()
            {
                let at = |len: usize| index.min(len);
                let position = at(store.texts.len());
                store
                    .texts
                    .splice(position..position, (0..count).map(|_| None));
                let position = at(store.images.len());
                store
                    .images
                    .splice(position..position, (0..count).map(|_| None));
                let position = at(store.nav.len());
                store
                    .nav
                    .splice(position..position, (0..count).map(|_| None));
                let position = at(store.flags.len());
                store
                    .flags
                    .splice(position..position, (0..count).map(|_| ItemFlags::NONE));
            }
        }
        Body::DeleteItems { index, count } => {
            let (index, count) = (*index as usize, *count as usize);
            if let Some(store) = common(cx, element) {
                let range = index
                    .checked_add(count)
                    .map(|end| index..end)
                    .filter(|range| !range.is_empty());
                if let Some(range) = range.clone().filter(|range| range.end <= store.texts.len()) {
                    store.texts.drain(range.clone());
                    if range.end <= store.images.len() {
                        store.images.drain(range.clone());
                    }
                    if range.end <= store.flags.len() {
                        store.flags.drain(range);
                    }
                }
                // Nav records from the index to the end are freed even when
                // the arguments are invalid.
                for record in store.nav.iter_mut().skip(index) {
                    *record = None;
                }
                if let Some(range) = range.filter(|range| range.end <= store.nav.len()) {
                    store.nav.drain(range);
                }
            }
        }
        Body::GetSourceText(query) => {
            if query.item_data {
                message.handled = true;
                query.point_size = None;
                let index = query.item as usize;
                if let Some(store) = common(cx, element)
                    && let Some(text) = store.texts.get(index)
                {
                    let text = text.clone();
                    if let Body::GetSourceText(query) = &mut message.body {
                        query.text = text;
                    }
                }
            }
        }
        Body::SetSourceText {
            item,
            item_data: true,
            text,
            ..
        } => {
            let (item, text) = (*item, text.clone());
            message.handled = true;
            if let Some(store) = common(cx, element)
                && let Some(slot) = store.texts.get_mut(item as usize)
            {
                *slot = text;
            }
        }
        Body::GetSourceImage(query) => {
            if query.item_data {
                message.handled = true;
                let index = query.item as usize;
                if let Some(store) = common(cx, element)
                    && let Some(path) = store.images.get(index)
                {
                    let path = path.clone();
                    if let Body::GetSourceImage(query) = &mut message.body {
                        query.path = path;
                    }
                }
            }
        }
        Body::SetSourceImage {
            item,
            item_data: true,
            path,
            ..
        } => {
            let (item, path) = (*item, path.clone());
            message.handled = true;
            if let Some(store) = common(cx, element)
                && let Some(slot) = store.images.get_mut(item as usize)
            {
                *slot = path;
            }
        }
        Body::GetItemCheck { index, .. } => {
            let index = *index;
            message.handled = true;
            let position = index as usize;
            let checked = common(cx, element).is_some_and(|store| {
                position < store.images.len()
                    && store
                        .flags
                        .get(position)
                        .is_some_and(|flags| flags.contains(ItemFlags::CHECKED))
            });
            message.body = Body::GetItemCheck { index, checked };
        }
        Body::SetItemCheck { index, checked } => {
            let (index, checked) = (*index as usize, *checked);
            if let Some(store) = common(cx, element)
                && index < store.images.len()
                && let Some(flags) = store.flags.get_mut(index)
            {
                flags.set(ItemFlags::CHECKED, checked);
            }
        }
        Body::GetItemEnabled { index, .. } => {
            let index = *index as usize;
            message.handled = true;
            let enabled = common(cx, element)
                .and_then(|store| store.flags.get(index).copied())
                .is_none_or(|flags| !flags.contains(ItemFlags::DISABLED));
            if let Body::GetItemEnabled { enabled: out, .. } = &mut message.body {
                *out = enabled;
            }
        }
        Body::SetItemEnabled { index, enabled } => {
            let (index, enabled) = (*index as usize, *enabled);
            if let Some(store) = common(cx, element)
                && let Some(flags) = store.flags.get_mut(index)
            {
                flags.set(ItemFlags::DISABLED, !enabled);
            }
        }
        Body::GetNavData { item, .. } => {
            let index = item.map(|item| item as usize);
            message.handled = true;
            let data = common(cx, element)
                .and_then(|store| index.and_then(|index| store.nav.get(index).cloned()))
                .flatten()
                .unwrap_or_default();
            if let Body::GetNavData { data: out, .. } = &mut message.body {
                *out = data;
            }
        }
        Body::SetNavData { item, data } => {
            let (item, data) = (*item, data.clone());
            message.handled = true;
            if let Some(store) = common(cx, element)
                && let Some(slot) = item.and_then(|item| store.nav.get_mut(item as usize))
            {
                *slot = Some(data);
            }
        }
        _ => {}
    }
}

fn list_item_checkable(cx: &Context<'_, '_>, element: ElementId) -> bool {
    matches!(
        cx.tree
            .property(element, &[props::list_item_checkable()], None),
        Some(PropValue::Bool(true))
    )
}

pub(super) fn on_list_item(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    class: ClassId,
    message: &mut Message,
) {
    let checkable = list_item_checkable(cx, element);
    match &mut message.body {
        Body::KeyDown(key) if !checkable => {
            let user = crate::message::FocusUser::User(key.user);
            if cx.runtime.focus.pressed(user).is_some() {
                message.handled = true;
            }
        }
        Body::GetValue { value } if !checkable => {
            *value = 0;
            message.handled = true;
        }
        Body::SetValue { .. } if !checkable => message.handled = true,
        Body::UpdateState { explicit, focused } => {
            let (explicit, focused) = (*explicit, *focused);
            message.handled = true;
            list_item_state(cx, element, class, checkable, explicit, focused);
        }
        Body::GetSourceText(query) => {
            if query.item != 0 {
                return;
            }
            let Some(parent) = cx.tree.parent(element) else {
                return;
            };
            let Some(index) = cx.control_index(element) else {
                return;
            };
            let association = query.association;
            let reply = cx.send_body(
                parent,
                Body::GetSourceText(SourceTextQuery {
                    item: index,
                    association,
                    item_data: true,
                    text: None,
                    point_size: None,
                    extended: false,
                    y_offset: 0.0,
                    style: 0,
                }),
            );
            message.handled = reply.handled;
            if let (Body::GetSourceText(out), Body::GetSourceText(answer)) =
                (&mut message.body, reply.body)
            {
                out.text = answer.text;
                out.point_size = answer.point_size;
            }
        }
        Body::GetSourceImage(query) => {
            if query.item != 0 {
                return;
            }
            let Some(parent) = cx.tree.parent(element) else {
                return;
            };
            let Some(index) = cx.control_index(element) else {
                return;
            };
            let association = query.association;
            let reply = cx.send_body(
                parent,
                Body::GetSourceImage(SourceImageQuery {
                    item: index,
                    association,
                    item_data: true,
                    force_reload: false,
                    path: None,
                    texture: None,
                }),
            );
            message.handled = reply.handled;
            if let (Body::GetSourceImage(out), Body::GetSourceImage(answer)) =
                (&mut message.body, reply.body)
            {
                out.force_reload = answer.force_reload;
                out.path = answer.path;
                out.texture = answer.texture;
            }
        }
        Body::FindTabStop(query) => {
            if !matches!(query.direction, TabStopKind::Next | TabStopKind::Previous) {
                return;
            }
            let direction = query.direction;
            message.handled = true;
            let Some(parent) = cx.tree.parent(element) else {
                return;
            };
            let reply = cx.send_body(
                parent,
                Body::FindTabStop(NavQuery {
                    direction,
                    source: parent,
                    target: None,
                    target_is_source: false,
                }),
            );
            if let Body::FindTabStop(answer) = reply.body {
                message.body = Body::FindTabStop(answer);
            }
        }
        _ => {}
    }
}

/// `XuiListItem_UpdateState` 0x9215cf10.
fn list_item_state(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    class: ClassId,
    checkable: bool,
    explicit: bool,
    focused: bool,
) {
    if cx.control_visual(element).is_none() {
        return;
    }

    let focused = if explicit {
        focused
    } else {
        cx.has_focus(element)
    };
    let enabled = cx.is_enabled(element);
    if !checkable && cx.is_pressed(element) {
        if enabled {
            let frames = if focused {
                StateFrames::PRESS
            } else {
                StateFrames::NORMAL_PRESS
            };
            let _ = cx.play_state_frames(element, frames);
        } else if let Some(visual) = cx.control_visual(element) {
            cx.play_named_frames(visual, StateFrames::PRESS_DISABLE);
        }
        return;
    }

    if !focused
        && let Some(parent) = cx.tree.parent(element)
        && cx.cur_item(parent).1 == Some(element)
    {
        let checked = cx.control_value(element) != 0;
        let frames = match (enabled, checked) {
            (true, false) => StateFrames::NORMAL_SEL,
            (true, true) => StateFrames::NORMAL_SEL_CHECK,
            (false, false) => StateFrames::NORMAL_SEL_DISABLE,
            (false, true) => StateFrames::NORMAL_SEL_CHECK_DISABLE,
        };
        let _ = cx.play_state_frames(element, frames);
        return;
    }

    let mut forwarded = Message::new(Body::UpdateState { explicit, focused });
    cx.send_to_base(element, class, &mut forwarded);
}

fn scroll_end_value(cx: &mut Context<'_, '_>, element: ElementId) -> ScrollEndState {
    match cx.class_state(element) {
        Some(ClassState::ScrollEnd { state }) => *state,
        _ => ScrollEndState::Normal,
    }
}

pub(super) fn on_scroll_end(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &message.body {
        Body::TimelineEnd { .. } => {
            if scroll_end_value(cx, element) == ScrollEndState::Scrolling {
                cx.notify_parent(element, NotifyKind::ScrollEndStateRequest);
            }
        }
        Body::UpdateState { .. } => {
            message.handled = true;
            let state = scroll_end_value(cx, element);
            scroll_end_play(cx, element, state);
        }
        Body::ScrollEndSetState { state } => {
            let state = *state;
            message.handled = true;
            if state == ScrollEndState::Scrolling || state != scroll_end_value(cx, element) {
                if let Some(ClassState::ScrollEnd { state: stored }) = cx.class_state(element) {
                    *stored = state;
                }
                scroll_end_play(cx, element, state);
            }
        }
        _ => {}
    }
}

/// `XuiScrollEnd_PlayState` 0x9215d3e0.
fn scroll_end_play(cx: &mut Context<'_, '_>, element: ElementId, state: ScrollEndState) {
    if cx.control_visual(element).is_none() {
        return;
    }

    let enabled = cx.is_enabled(element);
    match state {
        ScrollEndState::Normal => cx.play_focus_state_frames(element, false, false),
        ScrollEndState::More => {
            let frames = if enabled {
                StateFrames::SCROLL_MORE
            } else {
                StateFrames::SCROLL_MORE_DISABLE
            };
            let _ = cx.play_state_frames(element, frames);
        }
        ScrollEndState::Scrolling => {
            if enabled {
                let _ = cx.play_state_frames(element, StateFrames::SCROLLING);
            }
        }
    }
}

fn scroll_bar<'c>(
    cx: &'c mut Context<'_, '_>,
    element: ElementId,
) -> Option<&'c mut ScrollBarState> {
    match cx.class_state(element)? {
        ClassState::ScrollBar(state) => Some(state),
        _ => None,
    }
}

pub(super) fn on_scroll_bar(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    class: ClassId,
    message: &mut Message,
) {
    match &message.body {
        Body::CreateVisual => {
            if let Some(state) = scroll_bar(cx, element) {
                state.body = None;
                state.thumb = None;
            }
            let mut forwarded = Message::new(Body::CreateVisual);
            cx.send_to_base(element, class, &mut forwarded);
            message.handled = true;

            let visual = cx.control_visual(element);
            let body_id: Vec<u16> = "ScrollBarBody".encode_utf16().collect();
            let thumb_id: Vec<u16> = "ScrollBarThumb".encode_utf16().collect();
            let body = visual.and_then(|visual| cx.tree.child_by_id(visual, &body_id));
            let thumb = visual.and_then(|visual| cx.tree.child_by_id(visual, &thumb_id));
            if let Some(state) = scroll_bar(cx, element) {
                state.body = body;
                state.thumb = thumb;
            }
            update_thumb(cx, element);
        }
        Body::UpdateState { .. } => {
            message.handled = true;
            let _ = cx.play_state_frames(element, StateFrames::NORMAL);
        }
        Body::ScrollBarSetRange {
            count,
            top,
            visible,
        } => {
            let (count, top, visible) = (*count, *top, *visible);
            message.handled = true;
            if let Some(state) = scroll_bar(cx, element) {
                state.count = count;
                state.top = top;
                state.visible = visible;
            }
            update_thumb(cx, element);
        }
        Body::SizeChanged => {
            message.handled = true;
            update_thumb(cx, element);
        }
        _ => {}
    }
}

/// `XuiScrollBar_UpdateThumb` 0x9215d880 and `CalcThumbRect` 0x9215d578.
fn update_thumb(cx: &mut Context<'_, '_>, element: ElementId) {
    let Some(state) = scroll_bar(cx, element).copied() else {
        return;
    };
    let Some(thumb) = state.thumb else {
        return;
    };
    if state.count == 0 || state.visible == 0 {
        cx.set_show(thumb, false);
        return;
    }
    cx.set_show(thumb, true);

    let vertical = !matches!(
        cx.tree
            .property(element, &[props::scroll_bar_direction()], None),
        Some(PropValue::Unsigned(1))
    );
    let min_thumb = match cx
        .tree
        .property(element, &[props::scroll_bar_min_thumb()], None)
    {
        Some(PropValue::Unsigned(value)) => value as f32,
        _ => 10.0,
    };

    let (track_x, track_y, track_w, track_h) = match state.body.and_then(|body| cx.tree.get(body)) {
        Some(body) => (
            body.position().x,
            body.position().y,
            body.size().width,
            body.size().height,
        ),
        None => {
            let (w, h) = size(cx, element);
            (0.0, 0.0, w, h)
        }
    };
    let Some(thumb_element) = cx.tree.get(thumb) else {
        return;
    };
    let (thumb_pos, thumb_size) = (thumb_element.position(), thumb_element.size());

    let count = state.count as f32;
    let visible = state.visible as f32;
    let (mut x, mut y, w, h) = if vertical {
        let length = track_h / count * visible;
        (
            thumb_pos.x,
            track_y,
            thumb_size.width,
            track_h.min(min_thumb.max(length)),
        )
    } else {
        let length = track_w / count * visible;
        (
            track_x,
            thumb_pos.y,
            track_w.min(min_thumb.max(length)),
            thumb_size.height,
        )
    };

    if state.count > 1 && state.count > state.visible {
        let steps = (state.count - state.visible) as f32;
        // The top converts as unsigned, so no top item (-1) puts the thumb
        // at the end of the track.
        let top = state.top.unwrap_or(u32::MAX) as f32;
        if vertical {
            let per_item = (track_h - h) / steps;
            y += top * per_item;
            if y + h > track_y + track_h {
                y = track_y + track_h - h;
            }
        } else {
            let per_item = (track_w - w) / steps;
            x += top * per_item;
            if x + w > track_x + track_w {
                x = track_x + track_w - w;
            }
        }
    }

    let _ = cx.tree.set_property(
        thumb,
        &[props::position()],
        None,
        PropValue::Vector(Vector3::new(x, y, 0.0)),
    );
    cx.tree.set_bounds(thumb, w, h);
    cx.flush_size_changes();
}

#[cfg(test)]
mod tests;
