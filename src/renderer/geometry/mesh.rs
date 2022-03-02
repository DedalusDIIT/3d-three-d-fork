use crate::core::*;
use crate::renderer::*;

///
/// A triangle mesh where the mesh data is transfered to the GPU.
///
pub struct Mesh {
    /// Buffer with the position data, ie. `(x, y, z)` for each vertex
    position_buffer: VertexBuffer,
    /// Buffer with the normal data, ie. `(x, y, z)` for each vertex.
    normal_buffer: Option<VertexBuffer>,
    /// Buffer with the tangent data, ie. `(x, y, z)` for each vertex.
    tangent_buffer: Option<VertexBuffer>,
    /// Buffer with the uv coordinate data, ie. `(u, v)` for each vertex.
    uv_buffer: Option<VertexBuffer>,
    /// Buffer with the color data, ie. `(r, g, b)` for each vertex.
    color_buffer: Option<VertexBuffer>,
    /// Buffer with the index data, ie. three contiguous integers define the triangle where each integer is and index into the other vertex buffers.
    index_buffer: Option<ElementBuffer>,
    context: Context,
    aabb: AxisAlignedBoundingBox,
    aabb_local: AxisAlignedBoundingBox,
    transformation: Mat4,
    texture_transform: Mat3,
}

impl Mesh {
    ///
    /// Copies the per vertex data defined in the given [CpuMesh](crate::CpuMesh) to the GPU, thereby
    /// making it possible to render the mesh.
    ///
    pub fn new(context: &Context, cpu_mesh: &CpuMesh) -> ThreeDResult<Self> {
        #[cfg(debug_assertions)]
        cpu_mesh.validate()?;

        let position_buffer = VertexBuffer::new_with_static(context, &cpu_mesh.positions)?;
        let normal_buffer = if let Some(ref normals) = cpu_mesh.normals {
            Some(VertexBuffer::new_with_static(context, normals)?)
        } else {
            None
        };
        let tangent_buffer = if let Some(ref tangents) = cpu_mesh.tangents {
            Some(VertexBuffer::new_with_static(context, tangents)?)
        } else {
            None
        };
        let index_buffer = if let Some(ref indices) = cpu_mesh.indices {
            Some(match indices {
                Indices::U8(ind) => ElementBuffer::new_with(context, ind)?,
                Indices::U16(ind) => ElementBuffer::new_with(context, ind)?,
                Indices::U32(ind) => ElementBuffer::new_with(context, ind)?,
            })
        } else {
            None
        };
        let uv_buffer = if let Some(ref uvs) = cpu_mesh.uvs {
            Some(VertexBuffer::new_with_static(context, uvs)?)
        } else {
            None
        };
        let color_buffer = if let Some(ref colors) = cpu_mesh.colors {
            Some(VertexBuffer::new_with_static(context, colors)?)
        } else {
            None
        };
        let aabb = cpu_mesh.compute_aabb();
        Ok(Self {
            context: context.clone(),
            position_buffer,
            normal_buffer,
            tangent_buffer,
            index_buffer,
            uv_buffer,
            color_buffer,
            aabb,
            aabb_local: aabb.clone(),
            transformation: Mat4::identity(),
            texture_transform: Mat3::identity(),
        })
    }

    ///
    /// Get the texture transform applied to the uv coordinates of the model.
    ///
    pub fn texture_transform(&mut self) -> &Mat3 {
        &self.texture_transform
    }

    ///
    /// Set the texture transform applied to the uv coordinates of the model.
    ///
    pub fn set_texture_transform(&mut self, texture_transform: Mat3) {
        self.texture_transform = texture_transform;
    }

    fn vertex_shader_source(fragment_shader_source: &str) -> ThreeDResult<String> {
        let use_positions = fragment_shader_source.find("in vec3 pos;").is_some();
        let use_normals = fragment_shader_source.find("in vec3 nor;").is_some();
        let use_tangents = fragment_shader_source.find("in vec3 tang;").is_some();
        let use_uvs = fragment_shader_source.find("in vec2 uvs;").is_some();
        let use_colors = fragment_shader_source.find("in vec4 col;").is_some();
        Ok(format!(
            "{}{}{}{}{}{}{}",
            if use_positions {
                "#define USE_POSITIONS\n"
            } else {
                ""
            },
            if use_normals {
                "#define USE_NORMALS\n"
            } else {
                ""
            },
            if use_tangents {
                if fragment_shader_source.find("in vec3 bitang;").is_none() {
                    Err(CoreError::MissingBitangent)?;
                }
                "#define USE_TANGENTS\n"
            } else {
                ""
            },
            if use_uvs { "#define USE_UVS\n" } else { "" },
            if use_colors {
                "#define USE_COLORS\n"
            } else {
                ""
            },
            include_str!("../../core/shared.frag"),
            include_str!("shaders/mesh.vert"),
        ))
    }
}

impl Geometry for Mesh {
    fn aabb(&self) -> AxisAlignedBoundingBox {
        self.aabb
    }

    fn transformation(&self) -> Mat4 {
        self.transformation
    }

    fn render_with_material(
        &self,
        material: &dyn Material,
        camera: &Camera,
        lights: &[&dyn Light],
    ) -> ThreeDResult<()> {
        let fragment_shader_source =
            material.fragment_shader_source(self.color_buffer.is_some(), lights);
        self.context.program(
            &Self::vertex_shader_source(&fragment_shader_source)?,
            &fragment_shader_source,
            |program| {
                material.use_uniforms(program, camera, lights)?;
                program.use_uniform_block("Camera", camera.uniform_buffer());
                program.use_uniform_mat4("modelMatrix", &self.transformation)?;

                if program.requires_attribute("position") {
                    program.use_attribute_vec3("position", &self.position_buffer)?;
                }
                if program.requires_attribute("uv_coordinates") {
                    program.use_uniform_mat3("textureTransform", &self.texture_transform)?;
                    let uv_buffer = self
                        .uv_buffer
                        .as_ref()
                        .ok_or(CoreError::MissingMeshBuffer("uv coordinates".to_string()))?;
                    program.use_attribute_vec2("uv_coordinates", uv_buffer)?;
                }
                if program.requires_attribute("normal") {
                    let normal_buffer = self
                        .normal_buffer
                        .as_ref()
                        .ok_or(CoreError::MissingMeshBuffer("normal".to_string()))?;
                    program.use_attribute_vec3("normal", normal_buffer)?;
                    program.use_uniform_mat4(
                        "normalMatrix",
                        &self.transformation.invert().unwrap().transpose(),
                    )?;
                    if program.requires_attribute("tangent") {
                        let tangent_buffer = self
                            .tangent_buffer
                            .as_ref()
                            .ok_or(CoreError::MissingMeshBuffer("tangent".to_string()))?;
                        program.use_attribute_vec4("tangent", tangent_buffer)?;
                    }
                }
                if program.requires_attribute("color") {
                    let color_buffer = self
                        .color_buffer
                        .as_ref()
                        .ok_or(CoreError::MissingMeshBuffer("color".to_string()))?;
                    program.use_attribute_vec4("color", color_buffer)?;
                }
                if let Some(ref index_buffer) = self.index_buffer {
                    program.draw_elements(
                        material.render_states(),
                        camera.viewport(),
                        index_buffer,
                    );
                } else {
                    program.draw_arrays(
                        material.render_states(),
                        camera.viewport(),
                        self.position_buffer.count() as u32 / 3,
                    );
                }
                Ok(())
            },
        )
    }
}

impl GeometryMut for Mesh {
    fn set_transformation(&mut self, transformation: Mat4) {
        self.transformation = transformation;
        let mut aabb = self.aabb_local.clone();
        aabb.transform(&self.transformation);
        self.aabb = aabb;
    }
}
