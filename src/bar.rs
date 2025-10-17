use ab_glyph::{Font as _, FontVec, PxScale, PxScaleFont};
use wayland_client::protocol::{wl_buffer, wl_compositor, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{self, Layer};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    self, Anchor, KeyboardInteractivity,
};

use crate::draw_state::DrawState;
use crate::parser::Alignment;
use crate::pixels::{Color, Pixels};
use crate::token::Token;

pub struct Bar {
    running: bool,
    configured: bool,
    surface: wl_surface::WlSurface,
    shm: wl_shm::WlShm,
    buffer: wl_buffer::WlBuffer,
    pixels: Pixels,
    font: PxScaleFont<FontVec>,
}

// TODO: implement config using cli arguments
impl Bar {
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
        let font =
            FontVec::try_from_vec(include_bytes!("/usr/share/fonts/TTF/Iosevka-Custom.ttf").into())
                .unwrap();
        let font = font.into_scaled(scale);

        Self {
            running: true,
            configured: false,
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
        // skip if not configured yet
        if !self.configured {
            return;
        }

        // TODO: use the default bg color
        self.pixels.clear(Color::new(0, 0, 0, 0xFF));

        let mut l = Vec::new();
        let mut c = Vec::new();
        let mut r = Vec::new();
        let mut ptr = &mut l;

        // collect all tokens to their correct section
        for token in tokens {
            match token {
                Token::Alignment(Alignment::Left) => ptr = &mut l,
                Token::Alignment(Alignment::Center) => ptr = &mut c,
                Token::Alignment(Alignment::Right) => ptr = &mut r,
                _ => ptr.push(token),
            }
        }

        let l_start: f32 = 0.;
        let c_start = (self.pixels.width() as f32
            - c.iter().map(|t| t.px_width(&self.font)).sum::<f32>())
            / 2.;
        let r_start =
            self.pixels.width() as f32 - r.iter().map(|t| t.px_width(&self.font)).sum::<f32>();

        let assocs = [(l_start, l), (c_start, c), (r_start, r)];

        for (start, tokens) in assocs {
            let mut draw_state = DrawState::new(&mut self.pixels, &self.font, start);

            for token in tokens {
                match token {
                    Token::Text(text) => draw_state.draw_text(text),
                    Token::Fg(color) => draw_state.set_fg(color),
                    Token::Bg(color) => draw_state.set_bg(color),
                    Token::Upwards(size) => draw_state.draw_upwards_bar(size),
                    Token::Downwards(size) => draw_state.draw_downwards_bar(size),
                    Token::Alignment(..) => unreachable!("all alignments are already handled"),
                }
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

delegate_noop!(Bar: ignore wl_compositor::WlCompositor);
delegate_noop!(Bar: ignore wl_surface::WlSurface);
delegate_noop!(Bar: ignore wl_shm::WlShm);
delegate_noop!(Bar: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Bar: ignore wl_buffer::WlBuffer);
delegate_noop!(Bar: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for Bar {
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
            state.configured = true;
        }
    }
}
