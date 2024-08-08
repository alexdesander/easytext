use std::hash::{BuildHasherDefault, Hash};

use ahash::AHasher;
use etagere::{size2, Allocation, BucketedAtlasAllocator};
use fontdue::Metrics;
use lru::LruCache;
use wgpu::{
    AddressMode, BindGroup, BindGroupLayout, Device, Extent3d, FilterMode, Queue, Sampler,
    SamplerDescriptor, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    TextureView, TextureViewDescriptor,
};

#[derive(Debug, Clone)]
pub struct PreparedGlyph {
    pub metrics: Metrics,
    // Invisible characters don't have an allocation
    pub allocation: Option<Allocation>,
    bitmap: Vec<u8>,
}

pub struct Atlas<F: Eq + Hash + Copy> {
    pub size: u32,
    max_size: u32,
    allocator: BucketedAtlasAllocator,
    // (FontId, Size, GlyphKey) -> PreparedGlyph
    allocated: LruCache<(F, u16, u16), PreparedGlyph>,
    texture: Texture,
    texture_view: TextureView,
    texture_sampler: Sampler,
    pub texture_bind_group_layout: BindGroupLayout,
    pub texture_bind_group: BindGroup,
}

impl<F: Eq + Hash + Copy> Atlas<F> {
    pub fn new(device: &Device) -> Self {
        let size = 512.min(device.limits().max_texture_dimension_2d);
        let max_size = 8192.min(device.limits().max_texture_dimension_2d);

        let texture = device.create_texture(&TextureDescriptor {
            label: Some("EasyText Glyph Atlas Texture"),
            size: Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&TextureViewDescriptor::default());
        let texture_sampler = device.create_sampler(&SamplerDescriptor {
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            ..Default::default()
        });
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("EasyText Glyph Atlas Texture Bind Group Layout"),
            });
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&texture_sampler),
                },
            ],
            label: Some("EasyText Glyph Atlas Texture Bind Group"),
        });

        Self {
            size,
            max_size,
            allocator: BucketedAtlasAllocator::new(size2(size as i32, size as i32)),
            allocated: LruCache::unbounded_with_hasher(BuildHasherDefault::<AHasher>::default()),
            texture,
            texture_view,
            texture_sampler,
            texture_bind_group_layout,
            texture_bind_group,
        }
    }

    fn grow(&mut self, device: &Device, queue: &Queue) -> Result<(), ()> {
        let size = (self.size * 2).min(self.max_size);
        if self.size == size {
            return Err(());
        }
        self.size = size;
        self.allocator.clear();
        self.allocator.grow(size2(size as i32, size as i32));

        // Create new texture
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("EasyText Glyph Atlas Texture"),
            size: Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&TextureViewDescriptor::default());
        self.texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.texture_sampler),
                },
            ],
            label: Some("EasyText Glyph Atlas Texture Bind Group"),
        });
        // Copy all glyphs to new texture
        for (_, glyph) in &mut self.allocated {
            if glyph.metrics.width == 0 || glyph.metrics.height == 0 {
                continue;
            }
            let allocation = self
                .allocator
                .allocate(size2(
                    glyph.metrics.width as i32,
                    glyph.metrics.height as i32,
                ))
                .unwrap();
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: allocation.rectangle.min.x as u32,
                        y: allocation.rectangle.min.y as u32,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &glyph.bitmap,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(glyph.metrics.width as u32),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: glyph.metrics.width as u32,
                    height: glyph.metrics.height as u32,
                    depth_or_array_layers: 1,
                },
            );
            glyph.allocation = Some(allocation);
        }
        self.texture = texture;
        self.texture_view = texture_view;
        Ok(())
    }

    pub fn get(&mut self, font_id: F, size: u16, glyph_index: u16) -> Option<&PreparedGlyph> {
        self.allocated.get(&(font_id, size, glyph_index))
    }

    pub fn insert(
        &mut self,
        device: &Device,
        queue: &Queue,
        font_id: F,
        size: u16,
        glyph_index: u16,
        metrics: Metrics,
        bitmap: Vec<u8>,
    ) -> &PreparedGlyph {
        // Invisible character
        if metrics.width == 0 || metrics.height == 0 {
            self.allocated.put(
                (font_id.clone(), size, glyph_index),
                PreparedGlyph {
                    metrics,
                    allocation: None,
                    bitmap,
                },
            );
            return self.allocated.get(&(font_id, size, glyph_index)).unwrap();
        }
        // Visible character
        let allocation = loop {
            match self
                .allocator
                .allocate(size2(metrics.width as i32, metrics.height as i32))
            {
                Some(allocation) => {
                    break allocation;
                }
                None => {
                    if self.grow(device, queue).is_err() {
                        let Some(to_remove) = self.allocated.pop_lru() else {
                            panic!("Failed to allocate glyph");
                        };
                        if let Some(allocation) = to_remove.1.allocation {
                            self.allocator.deallocate(allocation.id);
                        }
                    }
                }
            }
        };
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: allocation.rectangle.min.x as u32,
                    y: allocation.rectangle.min.y as u32,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &bitmap,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(metrics.width as u32),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: metrics.width as u32,
                height: metrics.height as u32,
                depth_or_array_layers: 1,
            },
        );

        self.allocated.put(
            (font_id.clone(), size, glyph_index),
            PreparedGlyph {
                metrics,
                allocation: Some(allocation),
                bitmap,
            },
        );
        self.allocated.get(&(font_id, size, glyph_index)).unwrap()
    }
}
