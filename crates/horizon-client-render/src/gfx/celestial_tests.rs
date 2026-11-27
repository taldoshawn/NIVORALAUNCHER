//! Render the real celestial shader with terrain far planes on either side of
//! the sun. No game textures, prepared resources or player data are needed.
use wgpu::util::DeviceExt;

#[test]
fn sun_survives_terrain_far_plane_with_early_sky() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("Celestial regression adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = super::entity_shader();
    let parsed = naga::front::wgsl::parse_str(&source).unwrap();
    let span = parsed
        .types
        .iter()
        .find_map(|(_, ty)| {
            if ty.name.as_deref() != Some("Globals") {
                return None;
            }
            if let naga::TypeInner::Struct { span, .. } = ty.inner {
                Some(span)
            } else {
                None
            }
        })
        .unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Celestial regression"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let attrs = wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x2,2=>Uint32,3=>Unorm8x4,4=>Uint8x4,5=>Snorm8x4,6=>Float32x2];
    let pipeline = |entry| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(entry),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 44,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_sky_celestial"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let fixed = pipeline(super::CELESTIAL_VERTEX_ENTRY);
    let old = pipeline("vs_entity");
    let white = device.create_texture_with_data(
        &queue,
        &wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &[255; 4],
    );
    let white = white.create_view(&Default::default());
    let sampler = device.create_sampler(&Default::default());
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: span as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind = |pipeline: &wgpu::RenderPipeline| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&white),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&white),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        })
    };
    let (fixed_group, old_group) = (bind(&fixed), bind(&old));
    let vertices: Vec<u32> = [
        (-30f32, -30f32),
        (30., -30.),
        (30., 30.),
        (-30., -30.),
        (30., 30.),
        (-30., 30.),
    ]
    .into_iter()
    .flat_map(|(x, y)| {
        [
            x.to_bits(),
            y.to_bits(),
            (-100f32).to_bits(),
            0,
            0,
            2 | (4 << 16),
            u32::MAX,
            0,
            0,
            0,
            0,
        ]
    })
    .collect();
    let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let render = |pipeline: &wgpu::RenderPipeline, group: &wgpu::BindGroup| {
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 64 * 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let view = target.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.draw(0..6, 0..1);
        }
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(64),
                },
            },
            target.size(),
        );
        queue.submit([encoder.finish()]);
        read.slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let bytes = read.slice(..).get_mapped_range().unwrap();
        bytes.chunks_exact(4).filter(|p| p[0] > 200).count()
    };
    for far in [32f32, 64., 128., 1024.] {
        let mut globals = vec![0f32; span as usize / 4];
        globals[..16].copy_from_slice(
            &glam::Mat4::perspective_rh(90f32.to_radians(), 1.0, far, 0.05).to_cols_array(),
        );
        queue.write_buffer(&uniform, 0, bytemuck::cast_slice(&globals));
        let actual = render(&fixed, &fixed_group);
        assert!(actual > 100, "sun vanished with far plane {far}");
        let before = render(&old, &old_group);
        if far < 100. {
            assert_eq!(before, 0, "fixture must reproduce the clipped sun");
        } else {
            assert_eq!(actual, before, "in-range celestial appearance changed");
        }
    }
}
