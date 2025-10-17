use ab_glyph::{point, Font as _, FontVec, PxScale, PxScaleFont, ScaleFont as _};
use wayland_client::protocol::{wl_buffer, wl_compositor, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{self, Layer};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    self, Anchor, KeyboardInteractivity,
};

use crate::parser::Alignment;
use crate::pixels::{Color, Pixels};
use crate::token::Token;

pub struct State {
    running: bool,
    surface: wl_surface::WlSurface,
    shm: wl_shm::WlShm,
    buffer: wl_buffer::WlBuffer,
    pixels: Pixels,
    font: PxScaleFont<FontVec>,
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

        // TODO: make configurable
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

        // TODO: make configurable
        let scale = PxScale::from(24.);
        let font = FontVec::try_from_vec(include_bytes!("/usr/share/fonts/TTF/Iosevka-Custom.ttf").into()).unwrap();
        let font = font.into_scaled(scale);

        Self {
            running: true,
            surface,
            shm,
            buffer,
            pixels,
            font,
        }
    }

    pub fn keep_running(&self) -> bool {
        self.running
    }

    pub fn stop_running(&mut self) {
        self.running = false;
    }

    pub fn draw_tokens<'a>(&mut self, tokens: impl Iterator<Item = Token<'a>>) {
        self.pixels.clear();

        let mut l = Vec::new();
        let mut c = Vec::new();
        let mut r = Vec::new();
        let mut ptr = &mut l;

        for token in tokens {
            match token {
                Token::Alignment(Alignment::Left) => ptr = &mut l,
                Token::Alignment(Alignment::Center) => ptr = &mut c,
                Token::Alignment(Alignment::Right) => ptr = &mut r,
                _ => ptr.push(token),
            }
        }

        // TODO: calculate the starting positions of each section, and draw the content.
        //       implement a method for drawing some token at a given position on pixel buffer.

        for token in l {
            let Token::Text(text) = token else {
                continue;
            };

            let mut start_x = 0.;
            for c in text.chars() {
                let mut glyph = self.font.scaled_glyph(c);
                glyph.position = point(start_x, 0.);

                let h_advance = self.font.h_advance(glyph.id);

                let Some(outline) = self.font.outline_glyph(glyph) else {
                    start_x += h_advance;
                    continue;
                };

                let bounds = outline.px_bounds();

                outline.draw(|x, y, f| {
                    let x = bounds.min.x as i32 + x as i32;
                    let y = bounds.min.y as i32 + y as i32 + self.font.ascent() as i32;
                    if x < 0 || y < 0 {
                        return;
                    }

                    let x = x as u32;
                    let y = y as u32;

                    let f = (255. * f.clamp(0., 1.)).ceil() as u8;
                    let color = Color::new(f, f, f, 0xFF);

                    self.pixels.set(x, y, color);
                });

                start_x += h_advance;
            }
        }

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
        if let zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            proxy.ack_configure(serial);
            state.pixels = Pixels::new(width, height);
            let size = state.pixels.size() as i32;

            let pool = state
                .shm
                .create_pool(state.pixels.as_fd(), size, qhandle, ());
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
