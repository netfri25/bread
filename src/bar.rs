use ab_glyph::{Font as _, FontVec, PxScale, PxScaleFont};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::output::Output;
use crate::parser::Alignment;
use crate::pixels::Pixels;
use crate::token::Token;

pub struct Bar {
    running: bool,
    shm: wl_shm::WlShm,
    compositor: wl_compositor::WlCompositor,
    layer_shell: zwlr_layer_shell_v1::ZwlrLayerShellV1,
    font: PxScaleFont<FontVec>,
    outputs: Vec<Output>,
}

// TODO: implement config using cli arguments
impl Bar {
    pub fn new(
        compositor: wl_compositor::WlCompositor,
        shm: wl_shm::WlShm,
        layer_shell: zwlr_layer_shell_v1::ZwlrLayerShellV1,
    ) -> Self {
        // TODO: make configurable
        let scale = PxScale::from(24.);
        let font =
            FontVec::try_from_vec(include_bytes!("/usr/share/fonts/TTF/Iosevka-Custom.ttf").into())
                .unwrap();
        let font = font.into_scaled(scale);

        let outputs = Vec::new();

        Self {
            running: true,
            shm,
            font,
            compositor,
            layer_shell,
            outputs,
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
                Token::Alignment(Alignment::Left) => ptr = &mut l,
                Token::Alignment(Alignment::Center) => ptr = &mut c,
                Token::Alignment(Alignment::Right) => ptr = &mut r,
                _ => ptr.push(index),
            }
        }

        // width of left doesn't matter
        let l_width: f32 = 0.;

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
        let assocs = [
            (l_width, 0., l.as_slice()),  // start = (pixels - width) * 0
            (c_width, 0.5, c.as_slice()), // start = (pixels - width) * 0.5
            (r_width, 1., r.as_slice()),  // start = (pixels - width) * 1
        ];

        for output in &mut self.outputs {
            output.draw(tokens, &assocs, &self.font);
        }

        self.refresh();
    }

    fn refresh(&mut self) {
        self.outputs.iter_mut().for_each(Output::refresh);
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
            let output = Output::create(
                qhandle,
                &state.compositor,
                &state.layer_shell,
                &state.shm,
                proxy.clone(),
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
                let output = state.outputs.swap_remove(index);

                // destroy everything related to that output
                output.layer_surface.destroy(); // is the same as `proxy`
                output.wl_surface.destroy();
                output.buffer.destroy();
                // implicit `drop(output.pixels)`
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
