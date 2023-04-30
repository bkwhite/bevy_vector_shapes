use bevy::render::{
    render_resource::{
        encase::private::{
            ArrayMetadata, BufferMut, Metadata, RuntimeSizedArray, WriteInto, Writer,
        },
        ShaderType,
    },
    renderer::{RenderDevice, RenderQueue},
};
use std::{marker::PhantomData, num::NonZeroU64};
use wgpu::{BindingResource, Limits};

use super::{DynamicUniformBuffer, GpuListIndex, GpuListable};

// 1MB else we will make really large arrays on macOS which reports very large
// `max_uniform_buffer_binding_size`. On macOS this ends up being the minimum
// size of the uniform buffer as well as the size of each chunk of data at a
// dynamic offset.
const MAX_REASONABLE_UNIFORM_BUFFER_BINDING_SIZE: u32 = 1 << 20;

/// Similar to [`DynamicUniformBuffer`], except every N elements (depending on size)
/// are grouped into a batch as an `array<T, N>` in WGSL.
///
/// This reduces the number of rebindings required due to having to pass dynamic
/// offsets to bind group commands, and if indices into the array can be passed
/// in via other means, it enables batching of draw commands.
pub struct BatchedUniformBuffer<T: GpuListable> {
    uniforms: DynamicUniformBuffer<MaxCapacityArray<Vec<T>>>,
    temp: MaxCapacityArray<Vec<T>>,
    current_offset: u32,
    dynamic_offset_alignment: u32,
}

impl<T: GpuListable> BatchedUniformBuffer<T> {
    pub fn batch_size(limits: &Limits) -> usize {
        (limits
            .max_uniform_buffer_binding_size
            .min(MAX_REASONABLE_UNIFORM_BUFFER_BINDING_SIZE) as u64
            / T::min_size().get()) as usize
    }

    pub fn new(limits: &Limits) -> Self {
        let capacity = Self::batch_size(limits);
        let alignment = limits.min_uniform_buffer_offset_alignment;

        Self {
            uniforms: DynamicUniformBuffer::new_with_alignment(alignment as u64),
            temp: MaxCapacityArray(Vec::with_capacity(capacity), capacity),
            current_offset: 0,
            dynamic_offset_alignment: alignment,
        }
    }

    #[inline]
    pub fn size(&self) -> NonZeroU64 {
        self.temp.size()
    }

    pub fn clear(&mut self) {
        self.uniforms.clear();
        self.current_offset = 0;
        self.temp.0.clear();
    }

    pub fn push(&mut self, component: T) -> GpuListIndex<T> {
        let result = GpuListIndex {
            index: self.temp.0.len() as u32,
            dynamic_offset: Some(self.current_offset),
            element_type: PhantomData,
        };
        self.temp.0.push(component);
        if self.temp.0.len() == self.temp.1 {
            self.flush();
        }
        result
    }

    pub fn flush(&mut self) {
        self.uniforms.push(self.temp.clone());

        self.current_offset +=
            round_up(self.temp.size().get(), self.dynamic_offset_alignment as u64) as u32;

        self.temp.0.clear();
    }

    pub fn write_buffer(&mut self, device: &RenderDevice, queue: &RenderQueue) {
        if !self.temp.0.is_empty() {
            self.flush();
        }
        self.uniforms.write_buffer(device, queue);
    }

    #[inline]
    pub fn binding(&self) -> Option<BindingResource> {
        self.uniforms.binding()
    }
}

// ----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct MaxCapacityArray<T>(T, usize);

impl<T> ShaderType for MaxCapacityArray<T>
where
    T: ShaderType<ExtraMetadata = ArrayMetadata>,
{
    type ExtraMetadata = ArrayMetadata;

    const METADATA: Metadata<Self::ExtraMetadata> = T::METADATA;

    fn size(&self) -> ::core::num::NonZeroU64 {
        Self::METADATA.stride().mul(self.1.max(1) as u64).0
    }
}

impl<T> WriteInto for MaxCapacityArray<T>
where
    T: WriteInto + RuntimeSizedArray,
{
    fn write_into<B: BufferMut>(&self, writer: &mut Writer<B>) {
        debug_assert!(self.0.len() <= self.1);
        self.0.write_into(writer);
    }
}

#[inline]
fn round_up(v: u64, a: u64) -> u64 {
    ((v + a - 1) / a) * a
}
