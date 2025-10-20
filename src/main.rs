use std::io;
use std::os::unix::prelude::AsRawFd as _;

use clap::Parser as _;
use mio::Interest;
use mio::unix::SourceFd;
use wayland_client::{Connection, EventQueue};

const STDIN_TOKEN: mio::Token = mio::Token(0);
const WAYLAND_TOKEN: mio::Token = mio::Token(1);

mod bar;
mod collector;
mod draw_state;
mod output;
mod parser;
mod pixels;
mod token;
mod config;

use crate::bar::Bar;
use crate::collector::Collector;
use crate::config::Config;

#[macro_export]
macro_rules! bench {
    ($name:expr, $block:expr) => {{
        if cfg!(feature = "timing") {
            let name = $name;
            eprintln!("START: {} started", name);
            let start = ::std::time::Instant::now();
            let res = $block;
            let elapsed = start.elapsed();
            eprintln!("FINISH: {} took {:?}", name, elapsed);
            res
        } else {
            $block
        }
    }}
}

fn main() {
    let config = Config::parse();

    // implemented the dispatch using two steps:
    // 1. collect globals from registry (struct Collector)
    // 2. everything else (struct State)
    // this is a bit similar to the [builder pattern](https://rust-unofficial.github.io/patterns/patterns/creational/builder.html)
    let conn = Connection::connect_to_env().unwrap();

    // create a non blocking stdin reader
    let stdin = std::io::stdin();
    let stdin_fd = stdin.as_raw_fd();
    let mut reader = nonblock::NonBlockingReader::from_fd(stdin)
        .expect("can't open stdin for non-blocking read");

    let (mut state, mut event_queue) = init_bar(&conn, config);

    // used for polling efficiently from both stdin and the wayland socket
    let mut poll = mio::Poll::new().expect("unable to create Poll instance");

    // register stdin for polling
    let mut stdin_source = SourceFd(&stdin_fd);
    poll.registry()
        .register(&mut stdin_source, STDIN_TOKEN, Interest::READABLE)
        .expect("unable to register stdin");

    // the events collected by polling
    let mut events = mio::Events::with_capacity(16);

    // get the wayland socket fd
    let wayland_fd = {
        let read_guard = event_queue.prepare_read().unwrap();
        read_guard.connection_fd().as_raw_fd()
    };

    // register the wayland socket
    poll.registry()
        .register(&mut SourceFd(&wayland_fd), WAYLAND_TOKEN, Interest::READABLE)
        .unwrap();

    let mut buf = Vec::new();
    while state.keep_running() {
        // taken from https://docs.rs/wayland-client/latest/wayland_client/struct.EventQueue.html#integrating-the-event-queue-with-other-sources-of-events
        event_queue.flush().unwrap();
        event_queue.dispatch_pending(&mut state).unwrap();

        // wayland read guard
        let read_guard = event_queue.prepare_read().unwrap();

        // converted to Option so that it can be taken once without ownership
        let mut read_guard = Some(read_guard);

        // poll both the wayland socket and stdin
        let res = poll.poll(&mut events, None);

        match res {
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => panic!("POLLING ERROR: {err}"),
        };

        // go over all of the events that resulted from the poll
        for event in events.iter() {
            match event.token() {
                STDIN_TOKEN => {
                    buf.clear();
                    reader.read_available(&mut buf).unwrap();

                    // NOTE: the user should always be providing utf8 valid text
                    let input = unsafe { str::from_utf8_unchecked(&buf) };

                    let line = input.lines().last().unwrap_or_default();

                    if line.is_empty() {
                        state.stop_running()
                    }

                    let tokens: Vec<_> = bench!("parsing", parser::parse(line.trim()).collect());

                    bench!("draw", state.draw_tokens(&tokens));
                }

                WAYLAND_TOKEN => {
                    bench!("wayland", {
                        // since the read guard should be read only once, it's contained inside an
                        // Option so that it can be taken and used only once inside a loop.
                        let Some(read_guard) = read_guard.take() else {
                            unreachable!("too many wayland events")
                        };

                        if read_guard.read().is_ok() {
                            event_queue.dispatch_pending(&mut state).unwrap();
                        }
                    });
                }

                token => {
                    eprintln!("WARN: unexpected token from polling: {:?}", token)
                }
            }
        }
    }
}

fn init_bar(conn: &Connection, config: Config) -> (Bar, EventQueue<Bar>) {
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

    // request the registry for the bar as well, since it needs to keep track of new outputs
    display.get_registry(&qhandle, ());
    let mut bar = collector.collect(config);

    // this seems to be the right amount of dispatches needed to not miss the first input
    // it should let the bar initialize the surfaces and buffers needed
    for _ in 0..5 {
        event_queue.blocking_dispatch(&mut bar).unwrap();
    }

    (bar, event_queue)
}
