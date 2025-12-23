#![allow(unused)]

use std::{
    collections::HashMap,
    fmt::format,
    fs::{File, OpenOptions},
    os::fd::AsFd,
};

use drm::control::{Device, Event, Events, connector, property::EnumValue};
use udev::EventType;

#[derive(Debug)]
// Wrapper for a device node
struct Card(File);

// Traits form drm lib require AsFd require AsFd as constraint
impl AsFd for Card {
    fn as_fd(&self) -> std::os::unix::prelude::BorrowedFd<'_> {
        self.0.as_fd()
    }
}

// Should be implemented for object that acts as drm device
impl drm::Device for Card {}
// Should be implemented for object that acts as drm device, and allows for mode setting
impl drm::control::Device for Card {}
// Thanks to these two you can use drm functions like resource_handles() etc...

impl Card {
    fn open(path: &str) -> Self {
        let mut options = OpenOptions::new();
        options.read(true);
        options.write(true);
        Card(options.open(path).expect("open"))
    }
}

fn main() {
    let mut gpus = Gpus::new();
    loop {
        gpus.pool();
    }
}

fn rescan_connectors(card: &Card, cache: &mut HashMap<u32, connector::State>) {
    // Gets set of resource handles that this device currently controls (drm mode res struct)
    let res = card.resource_handles().expect("resource_handles");

    // Outputs like HDMI or DP ports on that card
    for &conn_handle in res.connectors() {
        let connector = card
            .get_connector(conn_handle, false)
            .expect("get_connector");

        let state = connector.state();
        let id: u32 = conn_handle.into();

        let name = format!("{:?}-{}", connector.interface(), connector.interface_id());

        let old = cache.insert(id, state);

        match (old, state) {
            (None, connector::State::Connected) => {
                println!("{} connected (initial)", name);
            }
            (None, connector::State::Disconnected) => {
                println!("{} disconnected (initial)", name);
            }

            (Some(prev), s) if prev == s => {
                // no change
            }

            (_, connector::State::Connected) => {
                println!("{} connected!", name);
                // TODO: modeset, create FB, redraw, etc
            }

            (_, connector::State::Disconnected) => {
                println!("{} disconnected!", name);
                // TODO: destroy FB, drop pipeline
            }

            _ => {}
        }
    }
}

struct Gpu {
    devnode: String,
    card: Card,
    known_states: HashMap<u32, connector::State>,
}

struct Gpus {
    gpus: Vec<Gpu>,
    socket: udev::MonitorSocket,
}

impl Gpus {
    fn new() -> Self {
        let mut enumerator = udev::Enumerator::new().unwrap();
        enumerator.match_subsystem("drm").unwrap();
        let mut gpus: Vec<Gpu> = Vec::new();

        for device in enumerator.scan_devices().unwrap() {
            if let Some(devnode) = device.devnode() {
                let path = devnode.to_string_lossy();
                if !path.contains("card") {
                    continue;
                }

                let card = Card::open(&path);
                let mut gpu = Gpu {
                    card,
                    known_states: HashMap::new(),
                    devnode: path.into_owned(),
                };

                rescan_connectors(&gpu.card, &mut gpu.known_states);
                gpus.push(gpu);
            }
        }

        let monitor = udev::MonitorBuilder::new().unwrap();
        let monitor = monitor.match_subsystem("drm").unwrap();

        let socket = monitor.listen().unwrap();

        Self { gpus, socket }
    }

    fn pool(&mut self) {
        for event in self.socket.iter() {
            let event_type = event.event_type();
            if event_type == EventType::Change || event_type == EventType::Add {
                for gpu in &mut self.gpus {
                    rescan_connectors(&gpu.card, &mut gpu.known_states);
                }
            }
        }
    }
}
