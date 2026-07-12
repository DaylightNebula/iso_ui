use std::{collections::VecDeque, marker::PhantomData};

use anarchy::macros::Getters;
use anyhow::ensure;
use bytemuck::Pod;
use magician_vgpu::{BufferContent, VirtualGpu};

/// The CPU type of an object whose shader equilvalent can
/// be loaded into a `TreeBuffer`.
pub trait TreeBufferElement: 'static {
    /// The shader type of this type used to represent the
    /// tree on the CPU side.
    type OutputType: TreeBufferContent<InputType = Self>;

    /// Get an iterator to references of the given output type
    /// that are children of this element.
    fn children(&self) -> impl Iterator<Item = &Self>;
}

/// The shader type of an object inside a buffer.
pub trait TreeBufferContent: BufferContent + Default + Pod + 'static {
    /// Extra inputs required when converting a CPU tree node into a GPU element.
    ///
    /// For UI shapes this is `crate::UIRenderResources`, which provides the chunked
    /// buffers used to store per-shape detail.
    type ConvertInput;

    /// The input type of a tree.  This should be seperate from
    /// the type used by shaders (self) as rust will not use the
    /// same types as shader to form a tree structure.
    type InputType: TreeBufferElement<OutputType = Self>;

    /// Create a new instance of this from a rust type that
    /// that represents the tree for rust, but not for the 
    /// shader.
    /// 
    /// If u32::MAX is given for either pointer, than the
    /// value it points to does not exist.
    fn new_gpu_type(
        vgpu: &VirtualGpu, 
        rust: &Self::InputType, 
        input: &Self::ConvertInput, 
        next_ptr: u32, 
        first_child_ptr: u32
    ) -> anyhow::Result<Self>;

    /// Set the next pointer in this object to a given index.
    /// The next pointer is an index in the array to an element
    /// that represents the next element on the same layer of 
    /// the tree with the same parent node.
    /// 
    /// If u32::MAX is given for the pointer, than the
    /// value it points to does not exist.
    fn set_next_ptr(&mut self, ptr: u32);
    
    /// Set the child pointer of this object to a given index.
    /// The child pointer is an index in the array that is the
    /// first child of the node self.
    /// 
    /// If u32::MAX is given for the pointer, than the
    /// value it points to does not exist.
    fn set_child_ptr(&mut self, ptr: u32);
}

/// GPU uniform buffer that stores a flattened UI element tree.
///
/// The tree is written as a contiguous array of `T` values with sibling
/// `next_ptr` links and parent `first_child_ptr` links wired up at upload time.
#[derive(Getters)]
pub struct TreeBuffer<T: TreeBufferContent> {
    buffer: wgpu::Buffer,
    max_size: usize,
    _phantom: PhantomData<T>
}

impl<T: TreeBufferContent> TreeBuffer<T> {
    /// Create a new `TreeBuffer` from the given tree_root.
    pub fn new(
        vgpu: &VirtualGpu,
        usage: wgpu::BufferUsages,
        max_size: u32,
    ) -> Self {
        let elem_size = std::mem::size_of::<T>() as u64;
        let buffer_size = elem_size * max_size as u64;

        let buffer = vgpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("TreeBuffer"),
            size: buffer_size,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            buffer,
            max_size: max_size as usize,
            _phantom: PhantomData,
        }
    }

    /// Update this tree buffer from the given tree_root.
    pub fn update(&self, vgpu: &VirtualGpu, tree_root: &T::InputType, input: &T::ConvertInput) -> anyhow::Result<()> {
        let data = flatten_tree::<T>(vgpu, tree_root, input)?;

        ensure!(
            data.len() <= self.max_size,
            "tree_root flattens to {} elements, exceeding buffer max_size of {}",
            data.len(),
            self.max_size
        );

        let bytes: &[u8] = bytemuck::cast_slice(&data);
        vgpu.queue().write_buffer(&self.buffer, 0, bytes);

        Ok(())
    }
}

/// Flatten a rust-side tree into GPU-layout elements, in the order they'll
/// be written to the buffer. The root is always index 0. Sibling `next_ptr`
/// chains and parent `first_child_ptr`s are wired up as we go.
fn flatten_tree<T: TreeBufferContent>(vgpu: &VirtualGpu, root: &T::InputType, input: &T::ConvertInput) -> anyhow::Result<Vec<T>> {
    let mut output: Vec<T> = vec![T::new_gpu_type(vgpu, root, input, std::u32::MAX, std::u32::MAX)?];

    // (input node, its index in `output`)
    let mut queue: VecDeque<(&T::InputType, usize)> = VecDeque::new();
    queue.push_back((root, 0));

    while let Some((node, parent_idx)) = queue.pop_front() {
        let mut children = node.children().peekable();
        if children.peek().is_none() {
            continue; // child_ptr stays u32::MAX from to_output()
        }

        let first_child_idx = output.len() as u32;
        output[parent_idx].set_child_ptr(first_child_idx);

        let mut prev_idx: Option<usize> = None;
        for child in children {
            let child_idx = output.len();
            output.push(T::new_gpu_type(vgpu, child, input, std::u32::MAX, std::u32::MAX)?);

            if let Some(p) = prev_idx {
                output[p].set_next_ptr(child_idx as u32);
            }
            prev_idx = Some(child_idx);

            queue.push_back((child, child_idx));
        }
    }

    Ok(output)
}
