use wayland_client::protocol::{wl_compositor, wl_registry, wl_shm};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1;

use crate::state::State;

// TODO: looks similar to builder pattern. maybe automate?
#[derive(Debug, Default)]
pub struct Collector {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
}

impl Collector {
    pub fn collect(self, qhandle: &QueueHandle<State>) -> State {
        State::new(
            qhandle,
            self.compositor.expect("wl_compositor not found"),
            self.shm.expect("wl_shm not found"),
            self.layer_shell.expect("zwlr_layer_shell_v1 not found"),
        )
    }
}

impl Dispatch<wl_registry::WlRegistry, QueueHandle<State>> for Collector {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: <wl_registry::WlRegistry as Proxy>::Event,
        state_qhandle: &QueueHandle<State>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(
                        registry.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(
                            name,
                            version,
                            state_qhandle,
                            (),
                        ),
                    )
                }

                "wl_compositor" => {
                    state.compositor = Some(registry.bind::<wl_compositor::WlCompositor, _, _>(
                        name,
                        version,
                        state_qhandle,
                        (),
                    ));
                }

                "wl_shm" => {
                    state.shm =
                        Some(registry.bind::<wl_shm::WlShm, _, _>(name, version, state_qhandle, ()))
                }

                _ => {}
            }
        }
    }
}
