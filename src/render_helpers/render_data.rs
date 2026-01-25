use smithay::backend::renderer::gles::{ffi, Capability, GlesFrame, GlesRenderer};

/// Extra renderer data used for custom drawing with gles FFI.
///
/// [`GlesRenderer`] creates these, but keeps them private, so we create our own.
pub struct RendererData {
    pub vbos: [u32; 2],
}

impl RendererData {
    pub fn init(renderer: &mut GlesRenderer) {
        let capabilities = renderer.capabilities();
        let vertices: &[ffi::types::GLfloat] = if capabilities.contains(&Capability::Instancing) {
            &INSTANCED_VERTS
        } else {
            &TRIANGLE_VERTS
        };

        let this = renderer
            .with_context(|gl| unsafe {
                let mut vbos = [0; 2];
                gl.GenBuffers(vbos.len() as i32, vbos.as_mut_ptr());
                gl.BindBuffer(ffi::ARRAY_BUFFER, vbos[0]);
                gl.BufferData(
                    ffi::ARRAY_BUFFER,
                    std::mem::size_of_val(vertices) as isize,
                    vertices.as_ptr() as *const _,
                    ffi::STATIC_DRAW,
                );
                gl.BindBuffer(ffi::ARRAY_BUFFER, vbos[1]);
                gl.BufferData(
                    ffi::ARRAY_BUFFER,
                    (std::mem::size_of::<ffi::types::GLfloat>() * OUTPUT_VERTS.len()) as isize,
                    OUTPUT_VERTS.as_ptr() as *const _,
                    ffi::STATIC_DRAW,
                );
                gl.BindBuffer(ffi::ARRAY_BUFFER, 0);

                Self { vbos }
            })
            .unwrap();

        renderer
            .egl_context()
            .user_data()
            .insert_if_missing(|| this);
    }

    pub fn get(renderer: &mut GlesRenderer) -> &Self {
        renderer.egl_context().user_data().get().unwrap()
    }

    pub fn get_from_frame<'a>(frame: &'a mut GlesFrame<'_, '_>) -> &'a Self {
        frame.egl_context().user_data().get().unwrap()
    }
}

/// Vertices for instanced rendering.
static INSTANCED_VERTS: [ffi::types::GLfloat; 8] = [
    1.0, 0.0, // top right
    0.0, 0.0, // top left
    1.0, 1.0, // bottom right
    0.0, 1.0, // bottom left
];

/// Vertices for rendering individual triangles.
const MAX_RECTS_PER_DRAW: usize = 10;
const TRIANGLE_VERTS: [ffi::types::GLfloat; 12 * MAX_RECTS_PER_DRAW] = triangle_verts();
const fn triangle_verts() -> [ffi::types::GLfloat; 12 * MAX_RECTS_PER_DRAW] {
    let mut verts = [0.; 12 * MAX_RECTS_PER_DRAW];
    let mut i = 0;
    loop {
        // Top Left.
        verts[i * 12] = 0.0;
        verts[i * 12 + 1] = 0.0;

        // Bottom left.
        verts[i * 12 + 2] = 0.0;
        verts[i * 12 + 3] = 1.0;

        // Bottom right.
        verts[i * 12 + 4] = 1.0;
        verts[i * 12 + 5] = 1.0;

        // Top left.
        verts[i * 12 + 6] = 0.0;
        verts[i * 12 + 7] = 0.0;

        // Bottom right.
        verts[i * 12 + 8] = 1.0;
        verts[i * 12 + 9] = 1.0;

        // Top right.
        verts[i * 12 + 10] = 1.0;
        verts[i * 12 + 11] = 0.0;

        i += 1;
        if i == MAX_RECTS_PER_DRAW {
            break;
        }
    }
    verts
}

/// Vertices for output rendering.
static OUTPUT_VERTS: [ffi::types::GLfloat; 8] = [
    -1.0, 1.0, // top right
    -1.0, -1.0, // top left
    1.0, 1.0, // bottom right
    1.0, -1.0, // bottom left
];
