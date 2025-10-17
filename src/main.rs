use std::os::unix::prelude::AsRawFd as _;

use mio::Interest;
use mio::unix::SourceFd;
use wayland_client::Connection;

const STDIN_TOKEN: mio::Token = mio::Token(0);
const WAYLAND_TOKEN: mio::Token = mio::Token(1);

mod collector;
mod parser;
mod pixels;
mod state;
mod token;

use crate::collector::Collector;

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

    while state.keep_running() {
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
                    if line.is_empty() {
                        state.stop_running()
                    }

                    for token in parser::parse(line.trim()) {
                        println!("token: {:?}", token);
                    }

                    let tokens = parser::parse(line.trim());
                    state.draw_tokens(tokens);
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
