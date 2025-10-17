use std::fs::File;
use std::io::Seek as _;
use std::ops::{Deref, DerefMut};
use std::os::unix::prelude::{AsFd, BorrowedFd};

use memmap::{MmapMut, MmapOptions};
use wayland_client::protocol::{wl_buffer, wl_compositor, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{self, Layer};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    self, Anchor, KeyboardInteractivity,
};

use crate::parser::Token;
use crate::pixels::{Color, Pixels};

pub struct State {
    running: bool,
    surface: wl_surface::WlSurface,
    shm: wl_shm::WlShm,
    buffer: wl_buffer::WlBuffer,
    pixels: Pixels,
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
        layer_surface.set_size(0, height);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_anchor(Anchor::Left | Anchor::Right | Anchor::Bottom);
        layer_surface.set_exclusive_zone(height as i32);
        surface.commit();

        let pixels = Pixels::new(1, height);
        let width = pixels.width() as i32;
        let stride = pixels.stride() as i32;
        let height = pixels.height() as i32;
        let size = pixels.size() as i32;

        let pool = shm.create_pool(pixels.as_fd(), size, qhandle, ());
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
            surface,
            shm,
            buffer,
            pixels,
        }
    }

    pub fn keep_running(&self) -> bool {
        self.running
    }

    pub fn stop_running(&mut self) {
        self.running = false;
    }

    pub fn draw_tokens<'a>(&mut self, tokens: impl Iterator<Item = Token<'a>>) {
        todo!();
        self.refresh();
    }

    pub fn draw_example(&mut self) {
        let color1 = Color::new(0x4b, 0x48, 0x47, 0xff);
        let color2 = Color::new(0x4e, 0x78, 0x37, 0xff);

        for y in 0..self.pixels.height() {
            for x in 0..self.pixels.width() {
                let sum = x / 50 + y / 50;
                let is_even = sum.is_multiple_of(2);
                let color = if is_even { color1 } else { color2 };
                self.pixels.set(x, y, color);
            }
        }

        self.refresh();
    }

    fn refresh(&mut self) {
        let width = self.pixels.width() as i32;
        let height = self.pixels.height() as i32;
        self.surface.attach(Some(&self.buffer), 0, 0);
        self.surface.damage(0, 0, width, height);
        self.surface.commit();
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
        if let zwlr_layer_surface_v1::Event::Configure { serial, width, height } = event {
            proxy.ack_configure(serial);
            state.pixels = Pixels::new(width, height);
            let size = state.pixels.size() as i32;

            let pool = state.shm.create_pool(state.pixels.as_fd(), size, qhandle, ());
            state.buffer.destroy();

            let stride = width * 4;
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
