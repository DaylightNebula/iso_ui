use std::{collections::LinkedList, hash::{BuildHasher, Hash, Hasher}, marker::PhantomData, ops::{Deref, Range}, sync::Arc};

use ahash::RandomState;
use anarchy::macros::Getters;
use anyhow::bail;
use bytemuck::Pod;
use magician_vgpu::{Buffer, BufferContent, VirtualGpu};
use mutual::{DashMap, RelaxedMutex, SharedData};

pub trait ChunkedBufferContent: BufferContent + Default + Hash + Pod + 'static {}

/// Allows chunks of data to be uploaded to a buffer without filling
/// the whole buffer at once.  The type of data being stored must have
/// some kind of default state that the buffer can be filled with by 
/// default, and then chunks of this can be populated one at a time.
pub struct ChunkedBuffer<T: ChunkedBufferContent>(Arc<ChunkedBufferInner<T>>);

impl <T: ChunkedBufferContent> ChunkedBuffer<T> {
    /// Create a new `ChunkedBuffer`.
    pub fn new(vgpu: &VirtualGpu, usage: wgpu::BufferUsages, max_size: u32) -> Self {
        let inner = ChunkedBufferInner { 
            buffer: vgpu.device().create_buffer(&wgpu::BufferDescriptor { 
                label: None, 
                size: max_size as u64 * std::mem::size_of::<T>() as u64, 
                usage, 
                mapped_at_creation: false
            }), 
            max_size, 
            unreserved: RelaxedMutex::new(LinkedList::from([0 .. max_size])), 
            reserved: DashMap::default(),
            _phantom: PhantomData::default()
        };
        Self(Arc::new(inner))
    }

    /// If the given data is a duplicate of some existing data in this buffer, the old
    /// handle will be returned, otherwise, the an empty spot in the buffer will be
    /// found, and then the data will be written to the internal `wgpu::Buffer`.
    /// 
    /// When a ChunkHandle produced from this function is dropped for the last time,
    /// its range will be returned to the unreserved memory list in this data structure,
    /// but the data will not be written to immeidately, but simply be overwritten as more
    /// data is added to this buffer.
    pub fn get(&self, vgpu: &VirtualGpu, data: &[T]) -> anyhow::Result<ChunkHandle<T>> {
        // get chunk hash
        let hash = hash128(data);

        // create lock now to avoid race with reserved list
        let mut unreserved = self.0.unreserved.lock_mut();

        // return previous chunk if one already exists
        let prev = self.0.reserved.get(&hash);
        if let Some(prev) = prev { 
            return Ok(ChunkHandle { inner: prev.clone(), buffer: self.0.clone() });
        }

        println!("Creating new handle for {}", hash);

        // find and take an empty slot 
        let mut cursor = unreserved.cursor_front_mut();
        let size = data.len() as u32;
        while let Some(range) = cursor.current() {
            let available = range.end - range.start;
            if available >= size {
                let start = range.start;

                // remove node at cursor if size will fill available space, otherwise, move start along
                if available == size {
                    cursor.remove_current();
                } else {
                    range.start += size;
                }

                // upload to buffer
                let offset = std::mem::size_of::<T>() as u64 * start as u64;
                let bytes = bytemuck::cast_slice(data);
                vgpu.queue().write_buffer(self.0.buffer(), offset, bytes);

                // create new node
                let inner = Arc::new(ChunkHandleInner {
                    start_idx: start,
                    size,
                    hash
                });
                self.0.reserved.insert(hash, inner.clone());
                return Ok(ChunkHandle { inner, buffer: self.0.clone() });
            }

            cursor.move_next();
        }

        bail!("Buffer to full to take buffer of size {}", size);
    }
}

#[derive(Getters)]
pub struct ChunkedBufferInner<T: ChunkedBufferContent> {
    buffer: wgpu::Buffer,
    max_size: u32,
    unreserved: RelaxedMutex<LinkedList<Range<u32>>>,
    reserved: DashMap<u128, Arc<ChunkHandleInner>>,
    _phantom: PhantomData<T>
}

/// Outer handle to a chunk inside a `ChunkedBuffer`.
/// The actual data is stored in `ChunkHandleInner` and
/// this is used to track how much a chunk is being used.
pub struct ChunkHandle<T: ChunkedBufferContent> {
    inner: Arc<ChunkHandleInner>,
    buffer: Arc<ChunkedBufferInner<T>>
}


impl <T: ChunkedBufferContent> Clone for ChunkHandle<T> {
    fn clone(&self) -> Self {
        ChunkHandle { 
            inner: self.inner.clone(), 
            buffer: self.buffer.clone() 
        }
    }
}

impl <T: ChunkedBufferContent> Deref for ChunkHandle<T> {
    type Target = ChunkHandleInner;
    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

impl <T: ChunkedBufferContent> Drop for ChunkHandle<T> {
    fn drop(&mut self) {
        // lock unreserved now to avoid race conditions with get
        let mut unreserved = self.buffer.unreserved.lock_mut();
        if Arc::strong_count(&self.inner) > 2 { return }

        // remove from reserved map
        println!("Dropping handle {:?}", self.inner);
        if self.buffer.reserved.remove(&self.hash).is_none() { return }

        // get cursor to return memory to unreserved memory
        let freed = self.start_idx..(self.start_idx + self.size);
        let mut cursor = unreserved.cursor_front_mut();

        // Advance to the first block whose start is >= freed.start.
        while cursor.current().map_or(false, |r| r.start < freed.start) {
            cursor.move_next();
        }

        // Does the previous block end exactly where `freed` starts?
        let left_merged = if let Some(prev) = cursor.peek_prev() {
            if prev.end == freed.start {
                prev.end = freed.end;
                true
            } else {
                false
            }
        } else {
            false
        };

        // Does the current/next block start exactly where `freed` ends?
        let right_touches = cursor.current().map_or(false, |r| r.start == freed.end);

        match (left_merged, right_touches) {
            (true, true) => {
                // prev was extended to freed.end, which equals current.start:
                // fuse prev and current into one block.
                let next_end = cursor.current().unwrap().end;
                cursor.remove_current();
                cursor.peek_prev().unwrap().end = next_end;
            }
            (true, false) => {
                // already absorbed into prev, nothing else to do
            }
            (false, true) => {
                cursor.current().unwrap().start = freed.start;
            }
            (false, false) => {
                cursor.insert_before(freed);
            }
        }
    }
}

/// Actual data belonging to a `ChunkHandle`
#[derive(Getters, Debug)]
pub struct ChunkHandleInner {
    start_idx: u32,
    size: u32,
    hash: u128
}

impl <T: ChunkedBufferContent> Buffer for ChunkedBufferInner<T> {
    type Type = T;
    fn buffer(&self) -> &wgpu::Buffer { &self.buffer }
    fn size(&self) -> u32 { self.max_size }
}

fn hash128<T: ChunkedBufferContent>(data: &[T]) -> u128 {
    let mut h1 = RandomState::with_seed(0x243F6A8885A308D3).build_hasher();
    let mut h2 = RandomState::with_seed(0xA4093822299F31D0).build_hasher();
    data.hash(&mut h1);
    data.hash(&mut h2);
    ((h1.finish() as u128) << 64) | (h2.finish() as u128)
}
