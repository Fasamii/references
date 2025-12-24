use std::collections::HashMap;

use drm::control::{Device, connector};

fn main() {
    let mut output = OutputManager::new();
    loop {
        let states = output.get_changed_connector_states();

        if !states.is_empty() {
            println!("changed = {states:?}");
        }
    }
}

#[derive(Debug)]
struct Card {
    file: std::fs::File,
    known_states: HashMap<u32, connector::State>,
}

impl std::os::fd::AsFd for Card {
    fn as_fd(&self) -> std::os::unix::prelude::BorrowedFd<'_> {
        self.file.as_fd()
    }
}

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

    fn get_all_changed_connector_states(&mut self) -> Vec<(u32, connector::State)> {
        let mut changed_states: Vec<(u32, connector::State)> = Vec::new();
        let resources = self.resource_handles().unwrap();

        for &conn_handle in resources.connectors() {
            let connector = self.get_connector(conn_handle, false).unwrap();
            let state = connector.state();

            let key: u32 = conn_handle.into();
            let old = self.known_states.insert(key, state);

            match (old, state) {
                (None, state) => changed_states.push((key, state)),
                (Some(old_state), state) if old_state != state => changed_states.push((key, state)),
                _ => (),
            }
        }
        changed_states
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

                let card = Card::new(path).unwrap();
                cards.push(card);
            }
        }

        Ok(Cards(cards))
    }

    fn get_all_cards_changed_connector_states(&mut self) -> Vec<(u32, connector::State)> {
        let mut changed_states: Vec<(u32, connector::State)> = Vec::new();
        for card in &mut self.0 {
            changed_states.extend(card.get_all_changed_connector_states());
        }
        changed_states
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

    fn get_changed_connector_states(&mut self) -> Vec<(u32, connector::State)> {
        let mut changed_states: Vec<(u32, connector::State)> = Vec::new();
        for event in self.socket.iter().take(10) {
            let event_type = event.event_type();
            if event_type == udev::EventType::Add || event_type == udev::EventType::Change {
                changed_states.extend(self.cards.get_all_cards_changed_connector_states());
            }
        }
        changed_states
    }
}
