use ab_glyph::{Font, PxScaleFont};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_output, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Dispatch, Proxy as _, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::Layer;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    Anchor, KeyboardInteractivity,
};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::bar::SectionInfo;
use crate::config::Config;
use crate::draw_state::DrawState;
use crate::pixels::{Color, Pixels};
use crate::token::Token;
use crate::bench;

// each wl_output needs it's own zwlr_layer_surface and wl_surface
// buffer can't shared between all surfaces, since some may have a different size
pub struct Output {
    pub configured: bool,
    pub layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    pub wl_surface: wl_surface::WlSurface,
    pub buffer: wl_buffer::WlBuffer,
    pub pixels: Pixels,
    pub output: wl_output::WlOutput,
    pub fg: Color,
    pub bg: Color,
}

impl Output {
    pub fn create<T>(
        qhandle: &QueueHandle<T>,
        compositor: &wl_compositor::WlCompositor,
        layer_shell: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        shm: &wl_shm::WlShm,
        output: wl_output::WlOutput,
        config: &Config,
    ) -> Self
    where
        T: 'static,
        T: Dispatch<wl_surface::WlSurface, ()>,
        T: Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()>,
        T: Dispatch<wl_shm::WlShm, ()>,
        T: Dispatch<wl_shm_pool::WlShmPool, ()>,
        T: Dispatch<wl_buffer::WlBuffer, ()>,
    {
        let wl_surface = compositor.create_surface(qhandle, ());
        let namespace = String::new();
        let layer_surface = layer_shell.get_layer_surface(
            &wl_surface,
            Some(&output),
            Layer::Top,
            namespace,
            qhandle,
            (),
        );

        layer_surface.set_size(0, config.height);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_anchor(Anchor::Left | Anchor::Right | Anchor::Bottom);
        layer_surface.set_exclusive_zone(config.height as i32);
        wl_surface.commit();

        let pixels = Pixels::new(1, config.height);
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

        let fg = config.fg;
        let bg = config.bg;

        Self {
            configured: false,
            layer_surface,
            wl_surface,
            buffer,
            pixels,
            output,
            fg,
            bg,
        }
    }

    pub fn draw(
        &mut self,
        tokens: &[Token],
        sections: &[SectionInfo; 3],
        font: &PxScaleFont<impl Font>,
    ) {
        // do not draw if not configured
        if !self.configured {
            return;
        }

        // TODO: use the default bg color
        bench!(format!("clear {}", self.output.id()), {
            self.pixels.clear(Color::new(0, 0, 0, 0xFF));
        });

        bench!(format!("render {}", self.output.id()), {
            let pixels_width = self.pixels.width() as f32;
            for section in sections {
                let start = (pixels_width - section.width) * section.mult;
                let mut draw_state = DrawState::new(&mut self.pixels, font, start, self.fg, self.bg);

                for &index in section.indices { let token = &tokens[index];
                    match token {
                        Token::Text(text) => draw_state.draw_text(text),
                        Token::Fg(color) => draw_state.set_fg(*color),
                        Token::Bg(color) => draw_state.set_bg(*color),
                        Token::Ramp(size) => draw_state.draw_ramp(*size),
                        Token::Section(..) => unreachable!("all sections are already handled"),
                    }
                }
            }
        });
    }

    pub fn refresh(&mut self, sections: &[SectionInfo; 3]) {
        if !self.configured {
            return;
        }

        self.wl_surface.attach(Some(&self.buffer), 0, 0);

        let height = self.pixels.height() as i32;
        let pixels_width = self.pixels.width() as f32;
        for section in sections {
            let start_x = (pixels_width - section.width) * section.mult;
            self.wl_surface.damage(start_x as i32, 0, section.width as i32, height);
        }

        self.wl_surface.commit();
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.layer_surface.destroy();
        self.wl_surface.destroy();
        self.buffer.destroy();
    }
}
