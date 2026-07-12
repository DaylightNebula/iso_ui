use anarchy::macros::Event;

#[derive(Event, Debug, Clone)]
pub struct UINodeStartHoverEvent { pub id: String }

#[derive(Event, Debug, Clone)]
pub struct UINodeEndHoverEvent { pub id: String }

#[derive(Event, Debug, Clone)]
pub struct UINodePressedEvent { pub id: String }

#[derive(Event, Debug, Clone)]
pub struct UINodeReleasedEvent { pub id: String }
