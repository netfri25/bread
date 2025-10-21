use ab_glyph::{Font as _, FontVec, PxScale, PxScaleFont};
use rust_fontconfig::{FcFontCache, FcPattern};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::bench;
use crate::config::Config;
use crate::output::Output;
use crate::parser::Section;
use crate::pixels::Pixels;
use crate::token::Token;

pub struct SectionInfo<'a> {
    pub width: f32,
    pub mult: f32,
    pub indices: &'a [usize],
}

pub struct Bar {
    running: bool,
    config: Config,
    shm: wl_shm::WlShm,
    compositor: wl_compositor::WlCompositor,
    layer_shell: zwlr_layer_shell_v1::ZwlrLayerShellV1,
    font: PxScaleFont<FontVec>,
    outputs: Vec<Output>,
    prev_widths: [f32; 3],
}

impl Bar {
    pub fn new(
        compositor: wl_compositor::WlCompositor,
        shm: wl_shm::WlShm,
        layer_shell: zwlr_layer_shell_v1::ZwlrLayerShellV1,
        mut config: Config,
    ) -> Self {
        let fc = FcFontCache::build();

        let pattern = if let Some(font) = config.font.take() {
            FcPattern {
                name: Some(font),
                ..Default::default()
            }
        } else {
            FcPattern::default()
        };

        let Some(m) = fc.query(&pattern, &mut Vec::new()) else {
            if let Some(name) = pattern.name {
                eprintln!("ERROR: no such font '{}'", name);
            } else {
                eprintln!("ERROR: no font available");
            }

            std::process::exit(1);
        };

        let font_data = fc.get_font_bytes(&m.id).expect("font should be accessible");

        let scale = PxScale::from(config.font_size as f32);
        let font = FontVec::try_from_vec(font_data).unwrap();
        let font = font.into_scaled(scale);

        let outputs = Vec::new();

        Self {
            running: true,
            shm,
            font,
            compositor,
            layer_shell,
            outputs,
            config,
            prev_widths: Default::default(),
        }
    }

    pub fn keep_running(&self) -> bool {
        self.running
    }

    pub fn stop_running(&mut self) {
        self.running = false;
    }

    pub fn draw_tokens(&mut self, tokens: &[Token]) {
        let mut l = Vec::new();
        let mut c = Vec::new();
        let mut r = Vec::new();
        let mut ptr = &mut l;

        // collect all token indices to their correct section
        for (index, token) in tokens.iter().enumerate() {
            match token {
                Token::Section(Section::Left) => ptr = &mut l,
                Token::Section(Section::Center) => ptr = &mut c,
                Token::Section(Section::Right) => ptr = &mut r,
                _ => ptr.push(index),
            }
        }

        // width of left is used for damage
        let l_width: f32 = l
            .iter()
            .map(|&index| tokens[index].px_width(&self.font))
            .sum();

        let c_width: f32 = c
            .iter()
            .map(|&index| tokens[index].px_width(&self.font))
            .sum();

        let r_width: f32 = r
            .iter()
            .map(|&index| tokens[index].px_width(&self.font))
            .sum::<f32>();

        // since each output has it's own width, the calculation of the starting pixel had to be
        // abstracted away.
        let l_section = SectionInfo {
            width: l_width,
            mult: 0., // start = (pixels - width) * 0
            indices: l.as_slice(),
        };

        let c_section = SectionInfo {
            width: c_width,
            mult: 0.5, // start = (pixels - width) * 0.5
            indices: c.as_slice(),
        };

        let r_section = SectionInfo {
            width: r_width,
            mult: 1., // start = (pixels - width) * 1
            indices: r.as_slice(),
        };

        let sections = [l_section, c_section, r_section];

        bench!("render", {
            for output in &mut self.outputs {
                output.draw(tokens, &sections, &self.font);
            }
        });

        bench!("refresh", self.refresh(sections));
    }

    fn refresh(&mut self, mut sections: [SectionInfo; 3]) {
        let new_widths = sections.each_ref().map(|s| s.width);
        let widths: [_; 3] = std::array::from_fn(|i| new_widths[i].max(self.prev_widths[i]));

        for (section, width) in sections.iter_mut().zip(widths) {
            section.width = width;
        }

        self.outputs.iter_mut().for_each(|o| o.refresh(&sections));
        self.prev_widths = new_widths;
    }
}

delegate_noop!(Bar: ignore wl_compositor::WlCompositor);
delegate_noop!(Bar: ignore wl_surface::WlSurface);
delegate_noop!(Bar: ignore wl_shm::WlShm);
delegate_noop!(Bar: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Bar: ignore wl_buffer::WlBuffer);
delegate_noop!(Bar: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

impl Dispatch<wl_registry::WlRegistry, ()> for Bar {
    fn event(
        _: &mut Self,
        proxy: &wl_registry::WlRegistry,
        event: <wl_registry::WlRegistry as Proxy>::Event,
        _: &(),
        _: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
            && interface == "wl_output"
        {
            proxy.bind::<wl_output::WlOutput, _, _>(name, version, qhandle, ());
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for Bar {
    fn event(
        state: &mut Self,
        proxy: &wl_output::WlOutput,
        event: <wl_output::WlOutput as Proxy>::Event,
        _: &(),
        _: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Done = event {
            if let Some(index) = state
                .outputs
                .iter()
                .position(|o| o.output.id() == proxy.id())
            {
                state.outputs.swap_remove(index);
            }

            let output = Output::create(
                qhandle,
                &state.compositor,
                &state.layer_shell,
                &state.shm,
                proxy.clone(),
                &state.config,
            );
            state.outputs.push(output);
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for Bar {
    fn event(
        state: &mut Self,
        proxy: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: <zwlr_layer_surface_v1::ZwlrLayerSurfaceV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Closed => {
                // find the related output
                let Some(index) = state
                    .outputs
                    .iter()
                    .position(|o| o.layer_surface.id() == proxy.id())
                else {
                    return;
                };

                // remove the output
                state.outputs.swap_remove(index);
            }

            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                // find the related output
                let Some(output) = state
                    .outputs
                    .iter_mut()
                    .find(|o| o.layer_surface.id() == proxy.id())
                else {
                    return;
                };

                // tell the proxy that you acknowledge the config request
                proxy.ack_configure(serial);

                // create new shared memory
                output.pixels = Pixels::new(width, height);
                let size = output.pixels.size() as i32;

                // make sure everything is initialized to bg color instead of transparent
                output.pixels.clear(output.bg);

                let pool = state
                    .shm
                    .create_pool(output.pixels.as_fd(), size, qhandle, ());
                output.buffer.destroy();

                let stride = width * 4;
                output.buffer = pool.create_buffer(
                    0,
                    width as i32,
                    height as i32,
                    stride as i32,
                    wl_shm::Format::Argb8888,
                    qhandle,
                    (),
                );

                // attach the new buffer to the surface
                output.wl_surface.attach(Some(&output.buffer), 0, 0);
                output.wl_surface.commit();
                output.configured = true;
            }

            _ => {}
        }
    }
}
