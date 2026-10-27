//! Compute skinning: each skinned placement is skinned once per frame into a vertex arena,
//! which the shadow and main passes then draw like any mesh.
//!
//! Per mesh, `register` builds a skin buffer (joints and weights per vertex) and a bind group
//! over it and the mesh's vertex buffer. Per frame, `queue` collects each placement's palette and
//! `dispatch` uploads the palettes, grows the arena to hold every placement's vertices and runs
//! one dispatch per placement. Layouts are in the GPU ABI reference.

use crate::types::{MeshEntry, Vertex};
use std::collections::HashMap;
use std::mem::size_of;
use wgpu::util::DeviceExt;

/// Threads per workgroup; matches `@workgroup_size` in `shaders/skin.wgsl`.
const WORKGROUP_SIZE: u32 = 64;

/// One vertex of the skin buffer; matches `SkinVertex` in `shaders/skin.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SkinVertexGpu {
    joints: [u32; 4],
    weights: [f32; 4],
}

/// Per-dispatch uniform; matches `SkinParams` in `shaders/skin.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SkinParamsGpu {
    vertex_count: u32,
    first_vertex: u32,
    first_joint: u32,
    pad: u32,
}

/// A mesh's skin on the GPU.
struct MeshSkin {
    bind_group: wgpu::BindGroup,
    vertex_count: u32,
    /// Highest joint index plus one: the shortest palette the skin can use.
    joint_count: usize,
}

/// A buffer that only grows.
struct GrowBuffer {
    buffer: wgpu::Buffer,
    size: u64,
}

pub(crate) struct Skinning {
    pipeline: wgpu::ComputePipeline,
    mesh_layout: wgpu::BindGroupLayout,
    frame_layout: wgpu::BindGroupLayout,
    /// Bytes between dispatch parameter slots, the device's uniform offset alignment.
    params_stride: u64,
    skins: HashMap<u32, MeshSkin>,
    /// This frame's palettes, one after another.
    palettes: Vec<[[f32; 4]; 3]>,
    /// This frame's dispatches as `(mesh, parameters)`.
    dispatches: Vec<(u32, SkinParamsGpu)>,
    /// Vertices queued so far this frame: the next placement's `first_vertex`.
    arena_vertices: u32,
    palette_buffer: Option<GrowBuffer>,
    params_buffer: Option<GrowBuffer>,
    arena: Option<GrowBuffer>,
    frame_bind_group: Option<wgpu::BindGroup>,
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Make `slot` hold at least `bytes`, replacing the buffer when it is too small.
/// Returns whether the buffer was replaced.
fn grow(
    device: &wgpu::Device,
    slot: &mut Option<GrowBuffer>,
    label: &str,
    usage: wgpu::BufferUsages,
    bytes: u64,
) -> bool {
    if slot.as_ref().is_some_and(|b| b.size >= bytes) {
        return false;
    }
    let size = bytes.next_power_of_two();
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage,
        mapped_at_creation: false,
    });
    *slot = Some(GrowBuffer { buffer, size });
    true
}

impl Skinning {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let mesh_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skin-mesh-bind-group-layout"),
            entries: &[storage_entry(0, true), storage_entry(1, true)],
        });
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skin-frame-bind-group-layout"),
            entries: &[
                storage_entry(0, true),
                storage_entry(1, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(size_of::<SkinParamsGpu>() as u64),
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skin-pipeline-layout"),
            bind_group_layouts: &[Some(&mesh_layout), Some(&frame_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("skin-shader"),
            source: wgpu::ShaderSource::Wgsl(crate::pipeline::skin_shader_source().into()),
        });
        let vertex_words = (size_of::<Vertex>() / size_of::<u32>()) as f64;
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("skin-pipeline"),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("cs_main"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[("VERTEX_WORDS", vertex_words)],
                ..Default::default()
            },
            cache: None,
        });
        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        Self {
            pipeline,
            mesh_layout,
            frame_layout,
            params_stride: (size_of::<SkinParamsGpu>() as u64).next_multiple_of(alignment),
            skins: HashMap::new(),
            palettes: Vec::new(),
            dispatches: Vec::new(),
            arena_vertices: 0,
            palette_buffer: None,
            params_buffer: None,
            arena: None,
            frame_bind_group: None,
        }
    }

    /// Upload the skin of `mesh`, one joint set and weight set per vertex. A skin that does
    /// not match the mesh's vertex count is logged and not registered.
    pub(crate) fn register(
        &mut self,
        device: &wgpu::Device,
        mesh: u32,
        entry: &MeshEntry,
        joints: &[[u16; 4]],
        weights: &[[f32; 4]],
    ) {
        let vertex_count = entry.vertex_count as usize;
        if vertex_count == 0 || joints.len() != vertex_count || weights.len() != vertex_count {
            tracing::error!(
                mesh,
                vertices = vertex_count,
                joints = joints.len(),
                weights = weights.len(),
                "skin does not match the mesh's vertices; mesh draws unskinned"
            );
            return;
        }
        let vertices: Vec<SkinVertexGpu> = joints
            .iter()
            .zip(weights)
            .map(|(j, w)| SkinVertexGpu {
                joints: j.map(u32::from),
                weights: *w,
            })
            .collect();
        let skin_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("skin-buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("skin-mesh-bind-group"),
            layout: &self.mesh_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: entry.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: skin_buffer.as_entire_binding(),
                },
            ],
        });
        let joint_count = joints
            .iter()
            .flatten()
            .map(|&j| usize::from(j) + 1)
            .max()
            .unwrap_or(1);
        self.skins.insert(
            mesh,
            MeshSkin {
                bind_group,
                vertex_count: entry.vertex_count,
                joint_count,
            },
        );
    }

    /// Forget the skin of `mesh`.
    pub(crate) fn unregister(&mut self, mesh: u32) {
        self.skins.remove(&mesh);
    }

    /// Queue skinning `mesh` with `palette` this frame. Returns the arena index of the
    /// placement's first vertex, or `None` (logged) when the mesh has no skin or the palette
    /// is too short for it.
    pub(crate) fn queue(&mut self, mesh: u32, palette: &[[[f32; 4]; 3]]) -> Option<u32> {
        let Some(skin) = self.skins.get(&mesh) else {
            tracing::error!(mesh, "skinned draw of a mesh with no skin; not drawn");
            return None;
        };
        if palette.len() < skin.joint_count {
            tracing::error!(
                mesh,
                joints = palette.len(),
                needed = skin.joint_count,
                "palette shorter than the skin's joints; not drawn"
            );
            return None;
        }
        let first_vertex = self.arena_vertices;
        self.dispatches.push((
            mesh,
            SkinParamsGpu {
                vertex_count: skin.vertex_count,
                first_vertex,
                first_joint: self.palettes.len() as u32,
                pad: 0,
            },
        ));
        self.arena_vertices += skin.vertex_count;
        self.palettes.extend_from_slice(palette);
        Some(first_vertex)
    }

    /// The frame arena the queued placements are skinned into; `None` before the first
    /// `dispatch` of a frame with skinned draws.
    pub(crate) fn arena(&self) -> Option<&wgpu::Buffer> {
        self.arena.as_ref().map(|a| &a.buffer)
    }

    /// Upload this frame's palettes and parameters, grow the arena to fit and record one
    /// dispatch per queued placement. Must run before any pass that draws the arena.
    pub(crate) fn dispatch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        if self.dispatches.is_empty() {
            return;
        }
        let palette_bytes: &[u8] = bytemuck::cast_slice(&self.palettes);
        let arena_bytes = u64::from(self.arena_vertices) * size_of::<Vertex>() as u64;
        let params_bytes = self.dispatches.len() as u64 * self.params_stride;
        let mut recreated = self.frame_bind_group.is_none();
        recreated |= grow(
            device,
            &mut self.palette_buffer,
            "skin-palette-buffer",
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            palette_bytes.len() as u64,
        );
        recreated |= grow(
            device,
            &mut self.params_buffer,
            "skin-params-buffer",
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            params_bytes,
        );
        recreated |= grow(
            device,
            &mut self.arena,
            "skin-arena",
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
            arena_bytes,
        );
        let (Some(palette), Some(params), Some(arena)) =
            (&self.palette_buffer, &self.params_buffer, &self.arena)
        else {
            unreachable!("grow fills every slot it is given");
        };

        queue.write_buffer(&palette.buffer, 0, palette_bytes);
        let stride = self.params_stride as usize;
        let mut params_data = vec![0u8; params_bytes as usize];
        for (slot, (_, p)) in params_data.chunks_mut(stride).zip(&self.dispatches) {
            slot[..size_of::<SkinParamsGpu>()].copy_from_slice(bytemuck::bytes_of(p));
        }
        queue.write_buffer(&params.buffer, 0, &params_data);

        if recreated {
            self.frame_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("skin-frame-bind-group"),
                layout: &self.frame_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: palette.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: arena.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &params.buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(size_of::<SkinParamsGpu>() as u64),
                        }),
                    },
                ],
            }));
        }
        let Some(frame) = &self.frame_bind_group else {
            unreachable!("the frame bind group is built above when missing");
        };

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("skin-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        for (i, (mesh, p)) in self.dispatches.iter().enumerate() {
            let Some(skin) = self.skins.get(mesh) else {
                continue;
            };
            pass.set_bind_group(0, &skin.bind_group, &[]);
            pass.set_bind_group(1, frame, &[(i * stride) as u32]);
            pass.dispatch_workgroups(p.vertex_count.div_ceil(WORKGROUP_SIZE), 1, 1);
        }
    }

    /// Drop this frame's queued placements; the arena and its buffers are kept.
    pub(crate) fn end_frame(&mut self) {
        self.palettes.clear();
        self.dispatches.clear();
        self.arena_vertices = 0;
    }
}

/// CPU reference of `shaders/skin.wgsl`: the position and normal of one vertex under the
/// blended joint matrix `M = sum(w_i * P[j_i])`. The position is `M * p`; the normal goes
/// by the cofactor of `M`'s 3x3, flipped on a negative determinant, and is normalised.
#[cfg(test)]
pub(crate) fn skin_vertex(
    position: [f32; 3],
    normal: [f32; 3],
    joints: [u16; 4],
    weights: [f32; 4],
    palette: &[[[f32; 4]; 3]],
) -> ([f32; 3], [f32; 3]) {
    use glam::{Mat3, Vec3, Vec4};

    let mut rows = [Vec4::ZERO; 3];
    for (&joint, weight) in joints.iter().zip(weights) {
        for (blended, row) in rows.iter_mut().zip(palette[usize::from(joint)]) {
            *blended += Vec4::from(row) * weight;
        }
    }
    let p = Vec3::from(position).extend(1.0);
    let skinned = Vec3::new(rows[0].dot(p), rows[1].dot(p), rows[2].dot(p));
    let [c0, c1, c2] = [
        Vec3::new(rows[0].x, rows[1].x, rows[2].x),
        Vec3::new(rows[0].y, rows[1].y, rows[2].y),
        Vec3::new(rows[0].z, rows[1].z, rows[2].z),
    ];
    let cofactor = Mat3::from_cols(c1.cross(c2), c2.cross(c0), c0.cross(c1));
    let flip = if c0.dot(cofactor.x_axis) < 0.0 {
        -1.0
    } else {
        1.0
    };
    let skinned_normal = (cofactor * Vec3::from(normal) * flip).normalize();
    (skinned.to_array(), skinned_normal.to_array())
}

#[cfg(test)]
mod tests {
    use super::skin_vertex;
    use crate::types::{DrawKey, Vertex, VertexSource};
    use glam::{Affine3A, Quat, Vec3};
    use proptest::prelude::*;
    use std::mem::size_of;

    /// A joint matrix as the palette buffer holds it: three rows of `[m_r0, m_r1, m_r2, t_r]`.
    fn rows(joint: Affine3A) -> [[f32; 4]; 3] {
        let (x, y, z, t) = (
            joint.matrix3.x_axis,
            joint.matrix3.y_axis,
            joint.matrix3.z_axis,
            joint.translation,
        );
        [
            [x.x, y.x, z.x, t.x],
            [x.y, y.y, z.y, t.y],
            [x.z, y.z, z.z, t.z],
        ]
    }

    fn close(actual: impl Into<Vec3>, expected: Vec3, eps: f32) -> bool {
        actual.into().abs_diff_eq(expected, eps)
    }

    fn direction(v: [f32; 3]) -> Vec3 {
        Vec3::from(v).normalize()
    }

    fn any_vec3() -> impl Strategy<Value = Vec3> {
        (-10.0f32..10.0, -10.0f32..10.0, -10.0f32..10.0).prop_map(|(x, y, z)| Vec3::new(x, y, z))
    }

    fn any_unit_vec3() -> impl Strategy<Value = Vec3> {
        any_vec3()
            .prop_filter("a direction needs length", |v| v.length() > 0.1)
            .prop_map(Vec3::normalize)
    }

    /// Four non-negative weights summing to one.
    fn any_weights() -> impl Strategy<Value = [f32; 4]> {
        prop::array::uniform4(0.0f32..1.0)
            .prop_filter("weights need a total", |w| w.iter().sum::<f32>() > 0.1)
            .prop_map(|w| {
                let total: f32 = w.iter().sum();
                w.map(|x| x / total)
            })
    }

    #[test]
    fn a_vertex_bound_to_a_turned_joint_turns_and_others_stay() {
        let turn = Affine3A::from_quat(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
        let palette = [rows(Affine3A::IDENTITY), rows(turn)];
        let (position, normal) = ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0]);

        let turned = skin_vertex(
            position,
            normal,
            [1, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            &palette,
        );
        let kept = skin_vertex(
            position,
            normal,
            [0, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            &palette,
        );

        assert!(close(turned.0, Vec3::Y, 1e-5), "position {:?}", turned.0);
        assert!(
            close(direction(turned.1), Vec3::Y, 1e-5),
            "normal {:?}",
            turned.1
        );
        assert!(close(kept.0, Vec3::X, 1e-5), "position {:?}", kept.0);
        assert!(
            close(direction(kept.1), Vec3::X, 1e-5),
            "normal {:?}",
            kept.1
        );
    }

    #[test]
    fn a_joint_translation_moves_the_position_but_not_the_normal() {
        let palette = [rows(Affine3A::from_translation(Vec3::new(0.0, 5.0, -2.0)))];

        let (position, normal) = skin_vertex(
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
            [0, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            &palette,
        );

        assert!(
            close(position, Vec3::new(1.0, 6.0, -1.0), 1e-5),
            "{position:?}"
        );
        assert!(close(direction(normal), Vec3::Y, 1e-5), "{normal:?}");
    }

    proptest! {
        #[test]
        fn the_identity_palette_leaves_every_vertex_unchanged(
            position in any_vec3(),
            normal in any_unit_vec3(),
            joints in prop::array::uniform4(0u16..4),
            weights in any_weights(),
        ) {
            let palette = [rows(Affine3A::IDENTITY); 4];

            let (skinned_position, skinned_normal) =
                skin_vertex(position.into(), normal.into(), joints, weights, &palette);

            prop_assert!(close(skinned_position, position, 1e-4), "{skinned_position:?} != {position:?}");
            prop_assert!(close(skinned_normal, normal, 1e-4), "{skinned_normal:?} != {normal:?}");
        }

        #[test]
        fn half_weights_on_two_joints_land_halfway(
            position in any_vec3(),
            normal in any_unit_vec3(),
            first in any_vec3(),
            second in any_vec3(),
            weight in 0.0f32..=1.0,
        ) {
            let palette = [
                rows(Affine3A::from_translation(second)),
                rows(Affine3A::from_translation(Vec3::splat(1000.0))),
                rows(Affine3A::from_translation(first)),
            ];

            let (skinned_position, skinned_normal) = skin_vertex(
                position.into(),
                normal.into(),
                [2, 0, 0, 0],
                [weight, 1.0 - weight, 0.0, 0.0],
                &palette,
            );

            let expected = position + first * weight + second * (1.0 - weight);
            prop_assert!(close(skinned_position, expected, 1e-3), "{skinned_position:?} != {expected:?}");
            prop_assert!(close(skinned_normal, normal, 1e-4), "{skinned_normal:?} != {normal:?}");
        }
    }

    #[test]
    fn skinned_normals_use_the_cofactor_of_the_blended_matrix() {
        let stretch = Affine3A::from_scale(Vec3::new(2.0, 1.0, 1.0));
        let palette = [rows(stretch), rows(Affine3A::IDENTITY)];
        let slanted = [1.0, 1.0, 0.0];

        let whole = skin_vertex(
            [1.0, 0.0, 0.0],
            slanted,
            [0, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            &palette,
        );
        let blended = skin_vertex(
            [1.0, 0.0, 0.0],
            slanted,
            [0, 1, 0, 0],
            [0.5, 0.5, 0.0, 0.0],
            &palette,
        );

        assert!(
            close(whole.0, Vec3::new(2.0, 0.0, 0.0), 1e-5),
            "{:?}",
            whole.0
        );
        assert!(
            close(
                direction(whole.1),
                Vec3::new(1.0, 2.0, 0.0).normalize(),
                1e-5
            ),
            "diag(2,1,1) bends (1,1,0) toward y, not x: {:?}",
            whole.1
        );
        assert!(
            close(blended.0, Vec3::new(1.5, 0.0, 0.0), 1e-5),
            "{:?}",
            blended.0
        );
        assert!(
            close(
                direction(blended.1),
                Vec3::new(2.0, 3.0, 0.0).normalize(),
                1e-5
            ),
            "diag(1.5,1,1) has cofactor diag(1,1.5,1.5): {:?}",
            blended.1
        );
    }

    #[test]
    fn a_skinned_draw_reads_the_arena_in_both_passes() {
        let stride = size_of::<Vertex>() as u64;
        let posed = DrawKey::skinned(3, 9, 10);
        let plain = DrawKey {
            mesh: 3,
            texture: 9,
            source: VertexSource::Mesh,
        };

        assert_eq!(posed.source, VertexSource::Arena { first_vertex: 10 });
        assert_eq!(
            posed.source.arena_bytes(5),
            Some(10 * stride..15 * stride),
            "the slice is the placement's own vertices in the frame arena"
        );
        assert_eq!(
            plain.source.arena_bytes(5),
            None,
            "mesh draws keep the mesh buffer"
        );
        assert_eq!(
            (posed.mesh, posed.texture),
            (3, 9),
            "the mesh still names the index buffer"
        );
    }
}
