# Bread
simple, non-interactlable, controlled from stdin, wayland status bar.

"a bar that reads" (b read).

### specifications
 - non-interactable (by design)
 - controlled from stdin with simple but powerful attributes
 - single threaded (by design)
 - efficient polling system

### build dependencies
 - [rust](https://rust-lang.org)
(doesn't need wayland development packages)

### Getting started
TBD

### Why?
 - learning wayland
 - creating my own status bar
 - separating rendering from information gathering and layout
 - flexible design
 - be the same on all monitors
 - single threaded with efficient async polling

the bar that made me almost satisfied is [zelbar](https://sr.ht/~novakane/zelbar/), and it encouraged me to create my own.
the only things I had issues with while trying out zelbar is that I wasn't able to draw boxes with a defined width and height (which I like for cpu usage), and it crashed if I closed the monitor it was on (which I do from time to time, because I use a laptop).

### TODO
 - [ ] multi monitor rendering
 - [x] input parsing
 - [x] rendering
 - [x] async polling
 - [ ] mention the other project that is able to provide content, as an example
 - [ ] cli arguments for simple config
    - [ ] default fg/bg colors
    - [ ] height
    - [ ] position (top/bottom)
    - [ ] font
    - [ ] font size
    - [ ] specific monitor (or all monitors by default)

### Special thanks
[zelbar](https://sr.ht/~novakane/zelbar/), which inspired this project.

[wayland-rs](https://github.com/Smithay/wayland-rs), which provides a really comfortable design for working with wayland.

[wayland](https://wayland.freedesktop.org) for providing an amazing protocol and architecture!
