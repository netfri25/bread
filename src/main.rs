use std::fs::File;
use std::io::Seek as _;
use std::os::unix::prelude::{AsFd, AsRawFd as _};

use mio::Interest;
use mio::unix::SourceFd;
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{self, Layer};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    self, Anchor, KeyboardInteractivity,
};

const STDIN_TOKEN: mio::Token = mio::Token(0);
const WAYLAND_TOKEN: mio::Token = mio::Token(1);

fn main() {
    // implemented the dispatch using two steps:
    // 1. collect globals from registry (struct Collector)
    // 2. everything else (struct State)
    // this is a bit similar to the [builder pattern](https://rust-unofficial.github.io/patterns/patterns/creational/builder.html)

    let conn = Connection::connect_to_env().unwrap();
    let display = conn.display();

    // collector event queue
    // there's a need for 2 event queues, since each event queue is depedent on the type of the
    // state, but `struct Collector` isn't the same as `struct State`
    let mut collector_event_queue = conn.new_event_queue();
    let collector_qhandle = collector_event_queue.handle();

    // state event queue
    let mut event_queue = conn.new_event_queue();
    let qhandle = event_queue.handle();

    // add the request of getting the registry to the event queue, and pass the state's queue
    // event handle to append all of the binding events
    display.get_registry(&collector_qhandle, qhandle.clone());

    // send the request, and react to events. this should collect all of the needed globals
    let mut collector = Collector::default();
    collector_event_queue
        .blocking_dispatch(&mut collector)
        .unwrap();

    // used for polling efficiently from both stdin and the wayland socket
    let mut poll = mio::Poll::new().expect("unable to create Poll instance");

    let stdin_fd = std::io::stdin().as_raw_fd();
    poll.registry()
        .register(&mut SourceFd(&stdin_fd), STDIN_TOKEN, Interest::READABLE)
        .expect("unable to register stdin");

    let mut events = mio::Events::with_capacity(16);

    let mut state = collector.collect(&qhandle);
    event_queue.roundtrip(&mut state).unwrap();

    while state.running {
        // taken from https://docs.rs/wayland-client/latest/wayland_client/struct.EventQueue.html#integrating-the-event-queue-with-other-sources-of-events
        event_queue.flush().unwrap();
        event_queue.dispatch_pending(&mut state).unwrap();

        // register the current wayland socket (`read_guard.connection_fd()` might return a different FD)
        let read_guard = event_queue.prepare_read().unwrap();
        let wayland_fd = read_guard.connection_fd().as_raw_fd();
        let mut wayland_source = SourceFd(&wayland_fd);
        poll.registry()
            .register(&mut wayland_source, WAYLAND_TOKEN, Interest::READABLE)
            .unwrap();

        let mut read_guard = Some(read_guard);

        // poll both the wayland socket and stdin
        poll.poll(&mut events, None).unwrap();

        // go over all of the events that resulted from the poll
        for event in events.iter() {
            match event.token() {
                STDIN_TOKEN => {
                    let mut line = String::new();
                    std::io::stdin().read_line(&mut line).unwrap();
                    println!("line: {:?}", line);
                    if line.is_empty() {
                        state.running = false;
                    }
                }

                WAYLAND_TOKEN => {
                    // since the read guard should be read only once, it's contained inside an
                    // Option so that it can be taken and used only once inside a loop.
                    let Some(read_guard) = read_guard.take() else {
                        unreachable!("too many wayland events")
                    };

                    read_guard.read().unwrap();
                    event_queue.dispatch_pending(&mut state).unwrap();
                }

                token => {
                    eprintln!("WARN: unexpected token from polling: {:?}", token)
                }
            }
        }

        // remove the wayland socket, since it might change
        poll.registry().deregister(&mut wayland_source).unwrap();
    }
}

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
