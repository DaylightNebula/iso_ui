use std::{hash::{BuildHasher, Hash, Hasher}, marker::PhantomData, ops::{Deref, Range}, sync::Arc};

use ahash::RandomState;
use anarchy::macros::Getters;
use anyhow::bail;
use bytemuck::Pod;
use magician_vgpu::{Buffer, BufferContent, VirtualGpu};
use mutual::{DashMap, RelaxedMutex, SharedData};

/// All content that may be stored in a chunked buffer, must implement this trait
/// to allow full interaction with the `ChunkedBuffer` system.
#[allow(unused)]
pub trait ChunkedBufferContent: BufferContent + Default + Hash + Pod + 'static {
    /// Assign the chunk length of the chunk this item is assigned too.
    fn set_chunk_len(&mut self, len: u32) {}

    /// Assign the index of this item in the chunk it is assigned too.
    fn set_idx(&mut self, idx: u32) {}

    /// Assign the index of the next item in the chunk this item was 
    /// assigned to, or none if this item is the last item in the chunk.
    fn set_next_idx(&mut self, idx: Option<u32>) {}

    /// Assign the index of the previous item in the chunk this item was 
    /// assigned to, or none if this item is the first item in the chunk.
    fn set_prev_idx(&mut self, idx: Option<u32>) {}
}

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
            unreserved: RelaxedMutex::new(vec![0 .. max_size]),
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
    pub fn get(&self, vgpu: &VirtualGpu, mut data: Box<[T]>) -> anyhow::Result<ChunkHandle<T>> {
        // get chunk hash
        let hash = hash128(&data);

        // create lock now to avoid race with reserved list
        let mut unreserved = self.0.unreserved.lock_mut();

        // return previous chunk if one already exists
        let prev = self.0.reserved.get(&hash);
        if let Some(prev) = prev { 
            return Ok(ChunkHandle { inner: prev.clone(), buffer: self.0.clone() });
        }

        // find and take an empty slot
        let size = data.len() as u32;
        for idx in 0..unreserved.len() {
            let range = &mut unreserved[idx];
            let available = range.end - range.start;
            if available >= size {
                let start = range.start;

                // remove slot at idx if size will fill available space, otherwise, move start along
                if available == size {
                    unreserved.remove(idx);
                } else {
                    range.start += size;
                }

                // update chunk items with length, idx, next idx, and prev idx information
                let mut chunk_idx = start;
                data.iter_mut().for_each(|item| {
                    item.set_chunk_len(size);
                    item.set_idx(chunk_idx);
                    item.set_prev_idx(if chunk_idx == 0 { None } else { Some(chunk_idx - 1) });
                    item.set_next_idx(if chunk_idx == size - 1 { None } else { Some(chunk_idx + 1) });
                    chunk_idx += 1;
                });

                // upload to buffer
                let offset = std::mem::size_of::<T>() as u64 * start as u64;
                let bytes = bytemuck::cast_slice(&data);
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
        }

        bail!("Buffer to full to take buffer of size {}", size);
    }
}

/// Shared backing state for a `ChunkedBuffer`.
#[derive(Getters)]
pub struct ChunkedBufferInner<T: ChunkedBufferContent> {
    buffer: wgpu::Buffer,
    max_size: u32,
    unreserved: RelaxedMutex<Vec<Range<u32>>>,
    reserved: DashMap<u128, Arc<ChunkHandleInner>>,
    _phantom: PhantomData<T>
}

impl <T: ChunkedBufferContent> Buffer for ChunkedBuffer<T> {
    type Type = T;
    fn buffer(&self) -> &wgpu::Buffer { &self.0.buffer }
    fn size(&self) -> u32 { self.0.max_size }
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
        if self.buffer.reserved.remove(&self.hash).is_none() { return }

        // find the index of the first block whose start is >= freed.start
        let freed = self.start_idx..(self.start_idx + self.size);
        let idx = unreserved.partition_point(|r| r.start < freed.start);

        // Does the previous block end exactly where `freed` starts?
        let left_merged = if idx > 0 && unreserved[idx - 1].end == freed.start {
            unreserved[idx - 1].end = freed.end;
            true
        } else {
            false
        };

        // Does the current/next block start exactly where `freed` ends?
        let right_touches = unreserved.get(idx).map_or(false, |r| r.start == freed.end);

        match (left_merged, right_touches) {
            (true, true) => {
                // prev was extended to freed.end, which equals current.start:
                // fuse prev and current into one block.
                let next_end = unreserved[idx].end;
                unreserved.remove(idx);
                unreserved[idx - 1].end = next_end;
            }
            (true, false) => {
                // already absorbed into prev, nothing else to do
            }
            (false, true) => {
                unreserved[idx].start = freed.start;
            }
            (false, false) => {
                unreserved.insert(idx, freed);
            }
        }
    }
}

/// Actual data belonging to a `ChunkHandle`.
///
/// `start_idx` is the index of the first of the chunk in the `ChunkedBuffer`,
/// `size` is the number of contiguous elements reserved, and `hash` is the
/// content hash used for deduplication.
#[derive(Getters, Debug)]
pub struct ChunkHandleInner {
    start_idx: u32,
    size: u32,
    hash: u128
}

/// Hash a slice of chunked-buffer elements for deduplication lookups.
fn hash128<T: ChunkedBufferContent>(data: &[T]) -> u128 {
    let mut h1 = RandomState::with_seed(0x243F6A8885A308D3).build_hasher();
    let mut h2 = RandomState::with_seed(0xA4093822299F31D0).build_hasher();
    data.hash(&mut h1);
    data.hash(&mut h2);
    ((h1.finish() as u128) << 64) | (h2.finish() as u128)
}
