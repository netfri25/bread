use std::fs::File;
use std::io::Seek as _;
use std::os::unix::prelude::AsFd;

use wayland_client::protocol::{wl_buffer, wl_compositor, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{self, Layer};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    self, Anchor, KeyboardInteractivity,
};

pub struct State {
    running: bool,
    file: File,
    surface: wl_surface::WlSurface,
    shm: wl_shm::WlShm,
    buffer: wl_buffer::WlBuffer,
}

// TODO: implement config using cli arguments
impl State {
    pub fn new(
        qhandle: &QueueHandle<Self>,
        compositor: wl_compositor::WlCompositor,
        shm: wl_shm::WlShm,
        layer_shell: zwlr_layer_shell_v1::ZwlrLayerShellV1,
    ) -> Self {
        let surface = compositor.create_surface(qhandle, ());

        let layer_surface =
            layer_shell.get_layer_surface(&surface, None, Layer::Top, "".into(), qhandle, ());

        let height = 24;
        layer_surface.set_size(0, height as u32);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_anchor(Anchor::Left | Anchor::Right | Anchor::Bottom);
        layer_surface.set_exclusive_zone(height);
        surface.commit();

        let width = 1;
        let stride = width * 4;
        let size = stride * height;

        let file = tempfile::tempfile().unwrap();

        let pool = shm.create_pool(file.as_fd(), size, qhandle, ());
        let buffer = pool.create_buffer(
            0,
            width,
            height,
            stride,
            wl_shm::Format::Argb8888,
            qhandle,
            (),
        );

        pool.destroy();

        Self {
            running: true,
            file,
            surface,
            shm,
            buffer,
        }
    }

    pub fn keep_running(&self) -> bool {
        self.running
    }

    pub fn stop_running(&mut self) {
        self.running = false;
    }
}

delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: <zwlr_layer_surface_v1::ZwlrLayerSurfaceV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            proxy.ack_configure(serial);

            let stride = width * 4;
            let size = stride * height;

            state.file.set_len(size as u64).unwrap();
            state.file.seek(std::io::SeekFrom::Start(0)).unwrap();
            draw(&mut state.file, (width, height));

            let pool = state
                .shm
                .create_pool(state.file.as_fd(), size as i32, qhandle, ());
            state.buffer.destroy();
            state.buffer = pool.create_buffer(
                0,
                width as i32,
                height as i32,
                stride as i32,
                wl_shm::Format::Argb8888,
                qhandle,
                (),
            );

            state.surface.attach(Some(&state.buffer), 0, 0);
            state.surface.commit();
        }
    }
}

fn draw(tmp: &mut File, (buf_x, buf_y): (u32, u32)) {
    use std::io::Write;
    let mut buf = std::io::BufWriter::new(tmp);
    for y in 0..buf_y {
        for x in 0..buf_x {
            let sum = x / 50 + y / 50;
            let is_even = sum % 2 == 0;

            // argb
            let color: u32 = if is_even { 0xff4b4847 } else { 0xff4e7837 };

            buf.write_all(&color.to_le_bytes()).unwrap();
        }
    }
    buf.flush().unwrap();
}
