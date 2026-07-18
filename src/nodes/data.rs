use std::sync::Arc;

use anarchy::macros::{Getters, Setters};
use derive_more::{Deref, DerefMut};
use magician_vgpu::glam::{Vec2, Vec4};
use vault::{Handle, TextureAsset};

use crate::SDFFont;

#[derive(Default, Debug, Clone, Deref, DerefMut)]
pub struct UINode {
    id: Option<String>,
    #[deref]
    #[deref_mut]
    style: Style,
    last_state: Option<LastState>,
    children: Vec<UINode>
}

impl UINode {
    /// Creates a new `UINode` with the given ID.  If you need to make
    /// a node without an ID, use the `default` function.
    pub fn new(id: String) -> Self {
        Self {
            id: Some(id),
            ..Default::default()
        }
    }

    /// Returns the ID of this node.
    pub fn id(&self) -> Option<&String> { self.id.as_ref() }

    /// Sets the ID of this node.  Returns true if this node had an ID previously.
    pub fn set_id(&mut self, id: String) -> bool { let had_id = self.id.is_some(); self.id = Some(id); had_id }

    /// Returns the current interaction state of this node.
    pub fn last_state(&self) -> Option<&LastState> { self.last_state.as_ref() }

    // /// Sets the interaction state of this node.
    // pub(crate) fn set_last_state(
    //     &mut self, 
    //     event_tracker: &EventTracker, 
    //     last_state: LastState
    // ) {
    //     // make sure this node has a previous state and ID
    //     if self.last_state.is_some() && self.id.is_some() {
    //         // get previous and new interaction
    //         let prev_interaction = self.last_state.as_ref().unwrap().interaction();
    //         let new_interaction = last_state.interaction();

    //         // if interactions have changed, pick event to broadcast
    //         if prev_interaction != new_interaction {
    //             let id = self.id.as_ref().unwrap().clone();
    //             match new_interaction {
    //                 Interaction::None => {
    //                     match prev_interaction {
    //                         Interaction::Hovered => event_tracker.broadcast_event(UINodeEndHoverEvent { id }),
    //                         _ => event_tracker.broadcast_event(UINodeReleasedEvent { id }),
    //                     }
    //                 },
    //                 Interaction::Hovered => {
    //                     match prev_interaction {
    //                         Interaction::Pressed => event_tracker.broadcast_event(UINodeReleasedEvent { id }),
    //                         _ => event_tracker.broadcast_event(UINodeStartHoverEvent { id })
    //                     }
    //                 },
    //                 Interaction::Pressed => event_tracker.broadcast_event(UINodePressedEvent { id })
    //             }
    //         }
    //     }

    //     self.last_state = Some(last_state);
    // }

    /// Returns a reference to the style of this node.
    pub fn style(&self) -> &Style { &self.style }

    /// Returns a mutable reference to the style of this node.
    pub fn style_mut(&mut self) -> &mut Style { &mut self.style }

    /// Sets the style of this node.
    pub fn set_style(&mut self, style: Style) { self.style = style; }

    /// Adds a child node to this node.
    pub fn add(&mut self, node: UINode) { self.children.push(node); }

    /// Adds all of nodes of an iterator as children to this node.
    pub fn add_all(&mut self, iter: impl Iterator<Item = UINode>) { 
        self.children.extend(iter); 
    }

    /// Attempts to remove a node of the given id from this nodes children.
    /// Returns an option to the removed node if it was found.
    pub fn remove(&mut self, id: &str) { self.children.retain(|node| node.id().map(|a| a != id).unwrap_or(false)) }

    /// Retains only the nodes that the given callback returns true for.
    pub fn retain(&mut self, callback: fn(&UINode) -> bool) {
        self.children.retain(callback);
    }

    /// Drains all children from this node.
    /// Returns an iterator to all drained children nodes.
    pub fn drain(&mut self) -> impl Iterator<Item = UINode> { self.children.drain(..) }

    /// Get all children
    pub fn children(&self) -> &[UINode] { &self.children }
}

#[derive(Setters, Getters, Default, Debug, Clone)]
pub struct Style {
    display: Display,
    position_type: PositionType,
    background: Background,
    overflow: Overflow,
    text: Option<Text>,
    aspect_ratio: Option<f32>, // only override width and height, 0 = match display aspect ratio
    width: Val,
    height: Val,
    min_width: Val,
    min_height: Val,
    max_width: Val,
    max_height: Val,
    margin: Rect,
    padding: Rect,
    border: Val,
    border_radius: RectCorners,
    border_color: Option<Vec4>
}

#[derive(Default, Debug, Clone, Copy)]
pub enum PositionType {
    #[default]
    Relative,
    Absolute(Rect)
}

#[derive(Default, Debug, Clone, Copy, Hash)]
pub enum Overflow {
    #[default]
    Clip,
    Allow
}

#[derive(Debug, Clone, Copy, Hash)]
pub enum Display {
    FlexColumn { vertical: Align, horizontal: Align },
    FlexRow { vertical: Align, horizontal: Align },
    Grid
}

impl Default for Display {
    fn default() -> Self {
        Self::FlexColumn { vertical: Align::Start, horizontal: Align::Start }
    }
}

#[derive(Default, Debug, Clone, Copy, Hash)]
pub enum Align {
    #[default]
    Start,
    End,
    Center
}

#[derive(Default, Debug, Clone)]
pub enum Background {
    #[default]
    Empty,
    Color(Vec4),
    Image(Handle<TextureAsset>)
}

#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Val {
    #[default]
    Auto,
    Px(f32),
    PercentWidth(f32),
    PercentHeight(f32)
}

impl Val {
    pub fn eval(&self, display_size: &[f32; 2]) -> Option<f32> {
        match self {
            Val::Auto => None,
            Val::Px(px) => Some(*px),
            Val::PercentWidth(percent) => Some(*percent * display_size[0] as f32),
            Val::PercentHeight(percent) => Some(*percent * display_size[1] as f32)
        }
    }
}

#[derive(Setters, Getters, Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Rect {
    top: Val,
    bottom: Val,
    left: Val,
    right: Val
}

impl Rect {
    pub fn new(top: Val, bottom: Val, left: Val, right: Val) -> Self {
        Self { top, bottom, left, right }
    }

    pub fn new_top(top: Val) -> Self {
        Self { top, ..Default::default() }
    }

    pub fn new_bottom(bottom: Val) -> Self {
        Self { bottom, ..Default::default() }
    }

    pub fn new_left(left: Val) -> Self {
        Self { left, ..Default::default() }
    }

    pub fn new_right(right: Val) -> Self {
        Self { right, ..Default::default() }
    }

    pub fn new_top_left(top: Val, left: Val) -> Self {
        Self { top, left, ..Default::default() }
    }

    pub fn new_top_right(top: Val, right: Val) -> Self {
        Self { top, right, ..Default::default() }
    }

    pub fn new_bottom_left(bottom: Val, left: Val) -> Self {
        Self { bottom, left, ..Default::default() }
    }

    pub fn new_bottom_right(bottom: Val, right: Val) -> Self {
        Self { bottom, right, ..Default::default() }
    }

    pub fn single(val: Val) -> Self {
        Self::new(val, val, val, val)
    }
}

#[derive(Setters, Getters, Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct RectCorners {
    top_left: Val,
    top_right: Val,
    bottom_left: Val,
    bottom_right: Val
}

impl RectCorners {
    pub fn new(top_left: Val, top_right: Val, bottom_left: Val, bottom_right: Val) -> Self {
        Self { top_left, top_right, bottom_left, bottom_right }
    }

    pub fn single(val: Val) -> Self {
        Self::new(val, val, val, val)
    }
}

#[derive(Getters, Default, Debug, Clone, Copy, PartialEq)]
pub struct LastState {
    pub position: Vec2,
    pub size: Vec2,
    pub interaction: Interaction
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Interaction {
    #[default]
    None,
    Hovered,
    Pressed
}

#[derive(Clone, Getters, Setters, Debug)]
pub struct Text {
    pub font: Arc<SDFFont>,
    pub content: String,
    pub color: Vec4,
    pub font_size: f32,
    pub horizontal_align: Align,
    pub vertical_align: Align
}
