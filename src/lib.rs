use std::hash::Hash;

use ahash::HashMap;
use area::TextArea;
use atlas::Atlas;
use bytemuck::{Pod, Zeroable};
use fontdue::{
    layout::{
        CoordinateSystem, HorizontalAlign, Layout, LayoutSettings, TextStyle, VerticalAlign,
        WrapStyle,
    },
    Font, FontSettings,
};
use wgpu::{
    util::DeviceExt, BindGroup, Device, PipelineLayoutDescriptor, Queue, RenderPass,
    RenderPipeline, RenderPipelineDescriptor, TextureFormat,
};

pub mod area;
mod atlas;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextAreaHandle {
    id: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct DebugLineVertex {
    pos: [f32; 2],
}

impl DebugLineVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;

        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct MetaInfo {
    window_size: [u32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct GlyphVertex {
    pos: [f32; 2],
    tex_coord: [f32; 2],
}

impl GlyphVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;

        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct EasyText<F: Eq + Hash + Copy> {
    window_size: [u32; 2],
    meta_info: MetaInfo,
    meta_info_buffer_bind_group: BindGroup,
    meta_info_buffer: wgpu::Buffer,
    atlas: Atlas<F>,
    debug_show_atlas: bool,
    debug_show_atlas_pipeline: RenderPipeline,
    debug_show_area_borders: bool,
    debug_show_area_borders_pipeline: RenderPipeline,
    debug_show_area_borders_vertex_buffer: Option<wgpu::Buffer>,
    debug_show_area_borders_vertex_count: u32,
    debug_show_area_borders_index_buffer: Option<wgpu::Buffer>,
    debug_show_area_borders_index_count: u32,

    fonts: HashMap<F, Font>,
    next_text_area_id: u32,
    text_areas: HashMap<TextAreaHandle, (TextArea<F>, Option<wgpu::Buffer>)>,
    dirty_text_areas: Vec<TextAreaHandle>,
    render_pipeline: RenderPipeline,
    layout: Layout,
}

impl<F: Eq + Hash + Copy> EasyText<F> {
    pub fn new(
        window_width: u32,
        window_height: u32,
        device: &Device,
        surface_format: TextureFormat,
    ) -> Self {
        let atlas = Atlas::new(device);
        let meta_info = MetaInfo {
            window_size: [window_width, window_height],
        };
        let meta_info_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("EasyText Meta Info Uniform Buffer"),
            contents: bytemuck::cast_slice(&[meta_info]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let meta_info_buffer_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("EasyText Meta Info Bind Group Layout"),
            });
        let meta_info_buffer_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &meta_info_buffer_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: meta_info_buffer.as_entire_binding(),
            }],
            label: Some("EasyText Meta Info Bind Group"),
        });

        // DEBUG SHOW ATLAS
        let debug_show_atlas_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("EasyText Debug Show Atlas Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("./shaders/debug_show_atlas.wgsl").into(),
            ),
        });
        let debug_show_atlas_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("EasyText Debug Show Atlas Pipeline Layout"),
                bind_group_layouts: &[&atlas.texture_bind_group_layout],
                push_constant_ranges: &[],
            });
        let debug_show_atlas_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("EasyText Render Pipeline"),
            layout: Some(&debug_show_atlas_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &debug_show_atlas_shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &debug_show_atlas_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // DEBUG DRAW AREA BORDERS
        let debug_show_area_borders_shader =
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("EasyText Debug Show TextArea Borders Shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("./shaders/debug_show_area_borders.wgsl").into(),
                ),
            });
        let debug_show_area_borders_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("EasyText Debug Show TextArea Borders Pipeline Layout"),
                bind_group_layouts: &[&meta_info_buffer_bind_group_layout],
                push_constant_ranges: &[],
            });
        let debug_show_area_borders_pipeline =
            device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some("EasyText Render Pipeline"),
                layout: Some(&debug_show_area_borders_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &debug_show_area_borders_shader,
                    entry_point: "vs_main",
                    buffers: &[DebugLineVertex::desc()],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &debug_show_area_borders_shader,
                    entry_point: "fs_main",
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::LineList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            });

        // RENDER PIPELINE
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("EasyText Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./shaders/shader.wgsl").into()),
        });
        let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("EasyText Atlas Render Pipeline Layout"),
            bind_group_layouts: &[
                &atlas.texture_bind_group_layout,
                &meta_info_buffer_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });
        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("EasyText Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[GlyphVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Self {
            window_size: [window_width, window_height],
            meta_info,
            meta_info_buffer,
            meta_info_buffer_bind_group,
            atlas,
            debug_show_atlas: false,
            debug_show_atlas_pipeline,
            debug_show_area_borders: false,
            debug_show_area_borders_pipeline,
            debug_show_area_borders_vertex_buffer: None,
            debug_show_area_borders_vertex_count: 0,
            debug_show_area_borders_index_buffer: None,
            debug_show_area_borders_index_count: 0,

            fonts: HashMap::default(),
            next_text_area_id: 0,
            text_areas: HashMap::default(),
            dirty_text_areas: Vec::new(),
            render_pipeline,
            layout: Layout::new(CoordinateSystem::PositiveYDown),
        }
    }

    pub fn resize(&mut self, queue: &Queue, new_width: u32, new_height: u32) {
        if new_width == 0 || new_height == 0 {
            return;
        }
        self.window_size = [new_width, new_height];
        self.meta_info = MetaInfo {
            window_size: [new_width, new_height],
        };
        queue.write_buffer(
            &self.meta_info_buffer,
            0,
            bytemuck::cast_slice(&[self.meta_info]),
        );
    }

    pub fn toggle_debug_show_atlas(&mut self) {
        self.debug_show_atlas = !self.debug_show_atlas;
    }

    pub fn toggle_debug_show_area_borders(&mut self) {
        self.debug_show_area_borders = !self.debug_show_area_borders;
    }

    pub fn add_font(&mut self, font_id: F, raw_file_content: Vec<u8>) {
        self.fonts.insert(
            font_id,
            Font::from_bytes(raw_file_content, FontSettings::default()).unwrap(),
        );
    }

    pub fn add_text_area(&mut self, text_area: TextArea<F>) -> TextAreaHandle {
        let id = self.next_text_area_id;
        self.next_text_area_id += 1;
        let handle = TextAreaHandle { id };
        self.text_areas.insert(handle, (text_area, None));
        if let Err(index) = self.dirty_text_areas.binary_search(&handle) {
            self.dirty_text_areas.insert(index, handle);
        }
        self.debug_show_area_borders_vertex_buffer = None;
        self.debug_show_area_borders_index_buffer = None;
        handle
    }

    pub fn remove_text_area(&mut self, handle: TextAreaHandle) {
        self.debug_show_area_borders_vertex_buffer = None;
        self.debug_show_area_borders_index_buffer = None;
        self.text_areas.remove(&handle);
    }

    pub fn text_area_mut(&mut self, handle: TextAreaHandle) -> Option<&mut TextArea<F>> {
        if let Err(index) = self.dirty_text_areas.binary_search(&handle) {
            self.dirty_text_areas.insert(index, handle);
        }
        self.debug_show_area_borders_vertex_buffer = None;
        self.debug_show_area_borders_index_buffer = None;
        self.text_areas.get_mut(&handle).map(|(area, _)| area)
    }

    pub fn text_area(&self, handle: TextAreaHandle) -> Option<&TextArea<F>> {
        self.text_areas.get(&handle).map(|(area, _)| area)
    }

    pub fn render(&mut self, device: &Device, queue: &Queue, render_pass: &mut RenderPass) {
        for handle in self.dirty_text_areas.drain(..) {
            let (area, vertex_buffer) = match self.text_areas.get_mut(&handle) {
                Some(area) => area,
                None => continue,
            };
            let font = self.fonts.get(&area.font).expect("Font not found");
            let layout_settings = LayoutSettings {
                x: area.x,
                y: area.y,
                max_width: Some(area.width),
                max_height: Some(area.height),
                horizontal_align: HorizontalAlign::Center,
                vertical_align: VerticalAlign::Middle,
                line_height: area.line_height_factor,
                wrap_style: WrapStyle::Word,
                wrap_hard_breaks: true,
            };
            self.layout.reset(&layout_settings);
            self.layout.append(
                &[font],
                &TextStyle {
                    text: &area.text,
                    px: area.size,
                    font_index: 0,
                    user_data: (),
                },
            );
            let size = area.size;
            let mut vertices = Vec::new();
            for glyph in self.layout.glyphs() {
                let prepared_glyph =
                    match self
                        .atlas
                        .get(area.font, size as u16, glyph.key.glyph_index)
                    {
                        Some(glyph) => glyph,
                        None => {
                            let (metrics, bitmap) =
                                font.rasterize_indexed(glyph.key.glyph_index, size);
                            self.atlas.insert(
                                device,
                                queue,
                                area.font,
                                area.size as u16,
                                glyph.key.glyph_index,
                                metrics,
                                bitmap,
                            )
                        }
                    };
                // Skip glyphs outside of the text area
                if glyph.y + (glyph.height as f32) < area.y || glyph.y > area.y + area.height {
                    continue;
                }
                if glyph.x + (glyph.width as f32) < area.x || glyph.x > area.x + area.width {
                    continue;
                }
                let allocation = match prepared_glyph.allocation {
                    Some(allocation) => allocation.rectangle,
                    None => continue,
                };
                let atlas_size = self.atlas.size as f32;
                vertices.extend_from_slice(&[
                    GlyphVertex {
                        pos: [glyph.x + area.left_offset, glyph.y + area.top_offset],
                        tex_coord: [
                            allocation.min.x as f32 / atlas_size,
                            allocation.min.y as f32 / atlas_size,
                        ],
                    },
                    GlyphVertex {
                        pos: [
                            glyph.x + glyph.width as f32 + area.left_offset,
                            glyph.y + area.top_offset,
                        ],
                        tex_coord: [
                            (allocation.min.x as usize + glyph.width) as f32 / atlas_size,
                            allocation.min.y as f32 / atlas_size,
                        ],
                    },
                    GlyphVertex {
                        pos: [
                            glyph.x + glyph.width as f32 + area.left_offset,
                            glyph.y + glyph.height as f32 + area.top_offset,
                        ],
                        tex_coord: [
                            (allocation.min.x as usize + glyph.width) as f32 / atlas_size,
                            (allocation.min.y as usize + glyph.height) as f32 / atlas_size,
                        ],
                    },
                    GlyphVertex {
                        pos: [glyph.x + area.left_offset, glyph.y + area.top_offset],
                        tex_coord: [
                            allocation.min.x as f32 / atlas_size,
                            allocation.min.y as f32 / atlas_size,
                        ],
                    },
                    GlyphVertex {
                        pos: [
                            glyph.x + glyph.width as f32 + area.left_offset,
                            glyph.y + glyph.height as f32 + area.top_offset,
                        ],
                        tex_coord: [
                            (allocation.min.x as usize + glyph.width) as f32 / atlas_size,
                            (allocation.min.y as usize + glyph.height) as f32 / atlas_size,
                        ],
                    },
                    GlyphVertex {
                        pos: [
                            glyph.x + area.left_offset,
                            glyph.y + glyph.height as f32 + area.top_offset,
                        ],
                        tex_coord: [
                            allocation.min.x as f32 / atlas_size,
                            (allocation.min.y as usize + glyph.height) as f32 / atlas_size,
                        ],
                    },
                ]);
            }
            let new_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Text Area Vertex Buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            *vertex_buffer = Some(new_vertex_buffer);
        }

        // Show text areas
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.atlas.texture_bind_group, &[]);
        render_pass.set_bind_group(1, &self.meta_info_buffer_bind_group, &[]);
        for (_, (_, vertex_buffer)) in self.text_areas.iter() {
            if let Some(vertex_buffer) = vertex_buffer {
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.draw(
                    0..(vertex_buffer.size() / std::mem::size_of::<GlyphVertex>() as u64) as u32,
                    0..1,
                );
            }
        }

        // DEBUG DRAW AREA BORDERS
        if self.debug_show_area_borders {
            if self.debug_show_area_borders_vertex_buffer.is_none() {
                let mut vertices = Vec::new();
                let mut indices = Vec::new();
                // Create vertex buffer
                for (i, (area, _)) in self.text_areas.values().enumerate() {
                    vertices.extend_from_slice(&[
                        DebugLineVertex {
                            pos: [area.x, area.y],
                        },
                        DebugLineVertex {
                            pos: [area.x + area.width, area.y],
                        },
                        DebugLineVertex {
                            pos: [area.x + area.width, area.y + area.height],
                        },
                        DebugLineVertex {
                            pos: [area.x, area.y + area.height],
                        },
                    ]);
                    let i = i as u32 * 4;
                    indices.extend_from_slice(&[i, i + 1, i + 1, i + 2, i + 2, i + 3, i + 3, i]);
                }
                let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Debug Show Area Borders Vertex Buffer"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Debug Show Area Borders Index Buffer"),
                    contents: bytemuck::cast_slice(&indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
                self.debug_show_area_borders_vertex_buffer = Some(vertex_buffer);
                self.debug_show_area_borders_vertex_count = vertices.len() as u32;
                self.debug_show_area_borders_index_buffer = Some(index_buffer);
                self.debug_show_area_borders_index_count = indices.len() as u32;
            }
            render_pass.set_pipeline(&self.debug_show_area_borders_pipeline);
            render_pass.set_bind_group(0, &self.meta_info_buffer_bind_group, &[]);
            render_pass.set_vertex_buffer(
                0,
                self.debug_show_area_borders_vertex_buffer
                    .as_ref()
                    .unwrap()
                    .slice(..),
            );
            render_pass.set_index_buffer(
                self.debug_show_area_borders_index_buffer
                    .as_ref()
                    .unwrap()
                    .slice(..),
                wgpu::IndexFormat::Uint32,
            );
            render_pass.draw_indexed(
                0..self.debug_show_area_borders_index_count,
                0,
                0..self.debug_show_area_borders_vertex_count,
            );
        }

        // DEBUG SHOW ATLAS
        if self.debug_show_atlas {
            render_pass.set_pipeline(&self.debug_show_atlas_pipeline);
            render_pass.set_bind_group(0, &self.atlas.texture_bind_group, &[]);
            render_pass.draw(0..4, 0..1);
        }
    }
}
