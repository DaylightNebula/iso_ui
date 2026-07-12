use anarchy::{Query, World, macros::Component};
use derive_more::{Deref, DerefMut};

use crate::SDFElement;

pub mod data;
pub mod events;
pub mod render;

pub use data::*;
pub use events::*;
pub use render::*;


/// Root of a UINode that is to be rendered as SDFs.
#[derive(Default, Debug, Deref, DerefMut, Component)]
pub struct UINodeSDFRoot(pub UINode);

/// `UISDFProvider` to provide UI information about
/// SDF shapes to be drawn.
#[derive(Default, Debug)]
pub struct UINodeSDFProvider;

impl UINodeSDFProvider {
    pub fn get(
        &self, 
        world: &World,
        display_size: &[f32; 2]
    ) -> Box<dyn Iterator<Item = SDFElement>> {
        // let vec = Query::<&UINodeSDFRoot>::new(world.database())
        //     .as_iter()
        //     .map(|node| sdf_render_ui_node(&*node, display_size))
        //     .collect::<Vec<_>>();

        let nodes = Query::<&UINodeSDFRoot>::new(world.database())
            .as_iter()
            .map(|a| a.clone())
            .collect::<Vec<_>>();
        let elements = render::layout_ui_nodes(&nodes, *display_size);

        Box::new(elements.into_iter())
    }
}
