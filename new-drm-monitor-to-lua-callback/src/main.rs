use drm::control::{Device, connector};
use std::collections::HashMap;

fn main() {
    let mut output = OutputManager::new();
    loop {
        let states = output.pool();
        if !states.is_empty() {
            println!("changed = {states:?}");
        }
    }
}

// Wrapper around card that drm lib can work with
#[derive(Debug)]
struct Card {
    file: std::fs::File,
    known_states: HashMap<u32, drm::control::connector::State>,
}

// Constraint for implementing other lib drm traits
impl std::os::fd::AsFd for Card {
    fn as_fd(&self) -> std::os::unix::prelude::BorrowedFd<'_> {
        self.file.as_fd()
    }
}

// Implementing traits that allow for Device and ControlDevice behaviors e.g.: resource_handles()
// method
impl drm::Device for Card {}
impl drm::control::Device for Card {}

impl Card {
    fn new(path: &str) -> Result<Self, std::io::Error> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        options.write(true);
        Ok(Self {
            file: options.open(path)?,
            known_states: HashMap::new(),
        })
    }

    fn list_changed_connection_states(&mut self) -> Vec<(u32, connector::State)> {
        let mut states: Vec<(u32, connector::State)> = Vec::new();

        let resources = self.resource_handles().unwrap();

        for &conn_handle in resources.connectors() {
            let connector = self.get_connector(conn_handle, false).unwrap();

            let state = connector.state();
            let id: u32 = conn_handle.into();

            let old = self.known_states.insert(id, state);
            match (old, state) {
                (None, state) => states.push((id, state)),
                (Some(old_state), state) if old_state != state => states.push((id, state)),
                _ => {}
            }
        }

        states
    }
}

#[derive(Debug)]
struct Cards(Vec<Card>);

impl Cards {
    fn new() -> Result<Self, ()> {
        let mut cards = Vec::new();

        let mut enumerator = udev::Enumerator::new().unwrap();
        enumerator.match_subsystem("drm").unwrap();
        for device in enumerator.scan_devices().unwrap() {
            if let Some(devnode) = device.devnode() {
                let path = devnode.to_str().unwrap();
                if !path.contains("card") {
                    continue;
                }

                cards.push(Card::new(path).unwrap());
            }
        }

        Ok(Cards(cards))
    }

    fn list_changed_connection_states(&mut self) -> Vec<(u32, connector::State)> {
        let mut states: Vec<(u32, connector::State)> = Vec::new();
        for card in &mut self.0 {
            states.append(&mut card.list_changed_connection_states());
        }
        states
    }
}

struct OutputManager {
    cards: Cards,
    socket: udev::MonitorSocket,
}

impl OutputManager {
    fn new() -> Self {
        let monitor = udev::MonitorBuilder::new().unwrap();
        let monitor = monitor.match_subsystem_devtype("drm", "drm_minor").unwrap();
        let socket = monitor.listen().unwrap();
        OutputManager {
            cards: Cards::new().unwrap(),
            socket,
        }
    }

    fn pool(&mut self) -> Vec<(u32, connector::State)> {
        let mut states: Vec<(u32, connector::State)> = Vec::new();
        for event in self.socket.iter().take(10) {
            let event_type = event.event_type();
            if event_type == udev::EventType::Add || event_type == udev::EventType::Change {
                states.append(&mut self.cards.list_changed_connection_states());
            }
        }
        states
    }
}
