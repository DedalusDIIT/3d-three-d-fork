// Entry point for non-wasm
#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
    run().await;
}

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use three_d::*;

use cgmath::SquareMatrix;
use log::error;
use once_cell::unsync::Lazy;
use three_d::{
    AxisAlignedBoundingBox, Camera, ColorTexture, Context, CpuMesh, DepthTexture, ElementBuffer, Geometry, Indices,
    Light, Mat4, Material, Positions, PostMaterial, Program, RenderStates, Vec3, VertexBuffer,
};


const SHADER_SRC: &str = include_str!("../assets/shaders/default.frag");

pub struct MainMaterial {
    render_texture: Rc<RefCell<Texture2D>>,
    fragment_shader_source: String,
}

impl MainMaterial {
    pub fn new(render_texture: Rc<RefCell<Texture2D>>) -> Self {
        let fragment_shader_source = SHADER_SRC.to_string();

        return Self {
            render_texture,
            fragment_shader_source,
        };
    }
}

impl Material for MainMaterial {
    fn fragment_shader_source(&self, _use_vertex_colors: bool, _lights: &[&dyn Light]) -> String {
        return self.fragment_shader_source.clone();
    }

    fn use_uniforms(&self, program: &Program, camera: &Camera, _lights: &[&dyn Light]) {
        let viewport = camera.viewport();
        program.use_uniform("viewportSize", Vec2::new(viewport.width as f32, viewport.height as f32));
        program.use_texture("renderTexture", &self.render_texture.borrow());
    }


    fn render_states(&self) -> RenderStates {
        return RenderStates {
            ..Default::default()
        };
    }

    fn material_type(&self) -> MaterialType {
        return MaterialType::Opaque;
    }
}

/*
Implementing custom mesh because provided three_d::Mesh does not allow to specify custom vertex shader
*/

#[allow(clippy::module_name_repetitions)]
pub struct CubeMesh {
    vertex_buffer: VertexBuffer,
    index_buffer: ElementBuffer,
    context: Context,
    aabb: AxisAlignedBoundingBox,
    transformation: Mat4,

    vertex_shader_source: String,
}

impl CubeMesh {
    pub fn new(context: &Context) -> Self {
        let cpu_mesh = cube();

        let vertex_buffer = VertexBuffer::new_with_data(context, cpu_mesh.positions.to_f32().as_slice());
        let index_buffer = cpu_mesh.indices.to_u32().map_or_else(
            || return ElementBuffer::new(context),
            |indices| return ElementBuffer::new_with_data(context, indices.as_slice()),
        );

        return Self {
            index_buffer,
            vertex_buffer,
            context: context.clone(),
            aabb: cpu_mesh.compute_aabb(),
            transformation: Mat4::identity(),
            vertex_shader_source: include_str!("../assets/shaders/default.vert").to_owned(),
        };
    }

    fn draw(&self, program: &Program, render_states: RenderStates, camera: &Camera) {
        program.use_uniform("camera.view", camera.view());
        program.use_uniform("camera.projection", camera.projection());
        program.use_uniform("modelMatrix", self.transformation);

        program.use_vertex_attribute("position", &self.vertex_buffer);
        program.draw_elements(render_states, camera.viewport(), &self.index_buffer);
    }

    pub fn set_transformation(&mut self, transformation: Mat4) {
        self.transformation = transformation;
    }
}

impl Geometry for CubeMesh {
    fn render_with_material(&self, material: &dyn Material, camera: &Camera, lights: &[&dyn Light]) {
        let fragment_shader_source = material.fragment_shader_source(false, lights);
        self.context
            .program(&self.vertex_shader_source, &fragment_shader_source, |program| {
                material.use_uniforms(program, camera, lights);
                self.draw(program, material.render_states(), camera);
            })
            .unwrap_or_else(|e| error!("Failed compiling shader: {:?}", e));
    }

    fn render_with_post_material(
        &self,
        material: &dyn PostMaterial,
        camera: &Camera,
        lights: &[&dyn Light],
        color_texture: Option<ColorTexture>,
        depth_texture: Option<DepthTexture>,
    ) {
        let fragment_shader_source = material.fragment_shader_source(lights, color_texture, depth_texture);
        self.context
            .program(&self.vertex_shader_source, &fragment_shader_source, |program| {
                material.use_uniforms(program, camera, lights, color_texture, depth_texture);
                self.draw(program, material.render_states(), camera);
            })
            .unwrap_or_else(|e| error!("Failed compiling shader: {:?}", e));
    }

    fn aabb(&self) -> AxisAlignedBoundingBox {
        return self.aabb;
    }
}

#[rustfmt::skip]
pub fn cube() -> CpuMesh {
    let positions = Positions::F32(vec![
        Vec3::new( -1.0, -1.0,  1.0),
        Vec3::new(1.0, -1.0,  1.0),
        Vec3::new(1.0,  1.0,  1.0),
        Vec3::new(-1.0,  1.0,  1.0),
        // back
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(1.0,  1.0, -1.0),
        Vec3::new(-1.0,  1.0, -1.0)
    ]);
    let indices = Indices::U8(vec![
        // front
        2, 1 ,0,
        0, 3, 2,
        // top
        6, 5, 1,
        1, 2, 6,
        // back
        5, 6, 7,
        7, 4, 5,
        // bottom
        3, 0, 4,
        4, 7, 3,
        // left
        1, 5, 4,
        4, 0, 1,
        // right
        6, 2, 3,
        3, 7, 6,
    ]);

    return CpuMesh {
        positions,
        indices,
        ..Default::default()
    };
}

pub async fn run() {
    let window = Window::new(WindowSettings {
        title: "Fog!".to_string(),
        max_size: Some((1280, 720)),
        ..Default::default()
    })
    .unwrap();
    let context = window.gl();

    let mut camera = Camera::new_perspective(
        window.viewport(),
        vec3(4.0, 4.0, 5.0),
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        degrees(45.0),
        0.1,
        1000.0,
    );
    let mut control = FlyControl::new(0.05);

    let mut loaded = three_d_asset::io::load_async(&["examples/assets/suzanne.obj"])
        .await
        .unwrap();

    let mut monkey =
        Model::<PhysicalMaterial>::new(&context, &loaded.deserialize("suzanne.obj").unwrap())
            .unwrap();
    monkey
        .iter_mut()
        .for_each(|m| m.material.render_states.cull = Cull::Back);

    let ambient = AmbientLight::new(&context, 0.4, Color::WHITE);
    let directional = DirectionalLight::new(&context, 2.0, Color::WHITE, &vec3(-1.0, -1.0, -1.0));

    // Fog
    let fog_effect = FogEffect {
        color: Color::new_opaque(200, 200, 200),
        density: 0.2,
        animation: 0.1,
    };
    let mut fog_enabled = true;

    // cube

    let cube_mesh = CubeMesh::new(&context);

    // main loop
    window.render_loop(move |mut frame_input| {
        let color_texture = Lazy::new(|| { return Rc::new(RefCell::new(Texture2D::new_empty::<[f32; 4]>(
            &context,
            frame_input.viewport.width,
            frame_input.viewport.height,
            Interpolation::Linear,
            Interpolation::Linear,
            None,
            Wrapping::ClampToEdge,
            Wrapping::ClampToEdge,
        )))});
        // let mut depth_texture = Lazy::new(|| { return DepthTexture2D::new::<f32>(
        //     &context,
        //     frame_input.viewport.width,
        //     frame_input.viewport.height,
        //     Wrapping::ClampToEdge,
        //     Wrapping::ClampToEdge,
        // )});




        let mut change = frame_input.first_frame;
        change |= camera.set_viewport(frame_input.viewport);
        change |= control.handle_events(&mut camera, &mut frame_input.events);

        for event in frame_input.events.iter() {
            match event {
                Event::KeyPress { kind, .. } => {
                    if *kind == Key::F {
                        fog_enabled = !fog_enabled;
                        change = true;
                        println!("Fog: {:?}", fog_enabled);
                    }
                }
                _ => {}
            }
        }

            // Draw the scene to a render target if a change has occured
        color_texture.borrow_mut().as_color_target(None)
            .clear(ClearState::default())
            .render(&camera, &monkey, &[&ambient, &directional]);

        if fog_enabled {

            let main_material = MainMaterial::new(color_texture.clone());


            // Apply fog nomatter if a change has occured since it contain animation.
            frame_input
                .screen()
                // .copy_from(
                //     ColorTexture::Single(&color_texture),
                //     DepthTexture::Single(&depth_texture),
                //     frame_input.viewport,
                //     WriteMask::default(),
                // )
                .write(|| {
                    // fog_effect.apply(
                    //     &context,
                    //     frame_input.accumulated_time,
                    //     &camera,
                    //     DepthTexture::Single(&depth_texture),
                    // );

                    cube_mesh.render_with_material(&main_material, &camera, &[]);
                });
        }
        // else if change {
        //     // If a change has happened and no fog is applied, copy the result to the screen
        //     frame_input.screen().copy_from_color(
        //         ColorTexture::Single(&color_texture.borrow()),
        //         frame_input.viewport,
        //         WriteMask::default(),
        //     );
        // }

        FrameOutput {
            swap_buffers: change || fog_enabled,
            ..Default::default()
        }
    });
}
