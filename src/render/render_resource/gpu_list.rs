use bevy::{
    prelude::*,
    render::{
        render_resource::{encase::private::WriteInto, ShaderSize, ShaderType, StorageBuffer},
        renderer::{RenderDevice, RenderQueue},
    },
};
use std::{marker::PhantomData, mem};
use wgpu::{
    BindGroupLayoutEntry, BindingResource, BindingType, BufferBindingType, Limits, ShaderStages,
};

use super::BatchedUniformBuffer;

/// Trait for types able to go in a [`GpuList`].
pub trait GpuListable: ShaderType + ShaderSize + WriteInto + Clone {}
impl<T: ShaderType + ShaderSize + WriteInto + Clone> GpuListable for T {}

/// Stores a list of elements to be transferred to the GPU and made accessible to shaders as a read-only array.
///
/// On platforms that support storage buffers, this is equivalent to [`StorageBuffer<Vec<T>>`].
/// Otherwise, this falls back to batched uniforms.
///
/// Other options for storing GPU-accessible data are:
/// * [`StorageBuffer`](crate::render_resource::StorageBuffer)
/// * [`DynamicStorageBuffer`](crate::render_resource::DynamicStorageBuffer)
/// * [`UniformBuffer`](crate::render_resource::UniformBuffer)
/// * [`DynamicUniformBuffer`](crate::render_resource::DynamicUniformBuffer)
/// * [`GpuList`](crate::render_resource::GpuList)
/// * [`BufferVec`](crate::render_resource::BufferVec)
/// * [`Texture`](crate::render_resource::Texture)
#[derive(Resource)]
pub enum GpuList<T: GpuListable> {
    Uniform(BatchedUniformBuffer<T>),
    Storage((StorageBuffer<Vec<T>>, Vec<T>)),
}

impl<T: GpuListable> GpuList<T> {
    pub fn new(device: &RenderDevice) -> Self {
        let limits = device.limits();
        if Self::use_fallback(&limits) {
            GpuList::Uniform(BatchedUniformBuffer::new(&limits))
        } else {
            GpuList::Storage((StorageBuffer::default(), Vec::new()))
        }
    }

    pub fn clear(&mut self) {
        match self {
            GpuList::Uniform(buffer) => buffer.clear(),
            GpuList::Storage((_, buffer)) => buffer.clear(),
        }
    }

    pub fn push(&mut self, value: T) -> GpuListIndex<T> {
        match self {
            GpuList::Uniform(buffer) => buffer.push(value),
            GpuList::Storage((_, buffer)) => {
                let index = buffer.len() as u32;
                buffer.push(value);
                GpuListIndex {
                    index,
                    dynamic_offset: None,
                    element_type: PhantomData,
                }
            }
        }
    }

    pub fn write_buffer(&mut self, device: &RenderDevice, queue: &RenderQueue) {
        match self {
            GpuList::Uniform(buffer) => buffer.write_buffer(device, queue),
            GpuList::Storage((buffer, vec)) => {
                buffer.set(mem::take(vec));
                buffer.write_buffer(device, queue);
            }
        }
    }

    pub fn binding_layout(
        binding: u32,
        visibility: ShaderStages,
        device: &RenderDevice,
    ) -> BindGroupLayoutEntry {
        BindGroupLayoutEntry {
            binding,
            visibility,
            ty: if Self::use_fallback(&device.limits()) {
                BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: Some(T::min_size()),
                }
            } else {
                BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(T::min_size()),
                }
            },
            count: None,
        }
    }

    pub fn binding(&self) -> Option<BindingResource> {
        match self {
            GpuList::Uniform(buffer) => buffer.binding(),
            GpuList::Storage((buffer, _)) => buffer.binding(),
        }
    }

    pub fn batch_size(device: &RenderDevice) -> Option<u32> {
        let limits = device.limits();
        if Self::use_fallback(&limits) {
            Some(BatchedUniformBuffer::<T>::batch_size(&limits) as u32)
        } else {
            None
        }
    }

    fn use_fallback(limits: &Limits) -> bool {
        return limits.max_storage_buffers_per_shader_stage > 0;
    }
}

/// An index into a [`GpuList`] for a given element.
#[derive(Component)]
pub struct GpuListIndex<T: GpuListable> {
    /// The index to use in a shader on the array.
    pub index: u32,
    /// The dynamic offset to use when binding the list from Rust.
    /// Only used on platforms that don't support storage buffers.
    pub dynamic_offset: Option<u32>,
    pub element_type: PhantomData<T>,
}
