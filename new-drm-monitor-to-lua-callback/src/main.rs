use drm::control::{Device, connector};
use std::io::Write;
use std::{collections::HashMap, hash::Hash};

const DEBOUNCE_MS: u64 = 100;

fn main() {
    let mut output = OutputManager::new().unwrap();
    println!("initial states : {:?}", output.connectors);
    loop {
        let states = output.update_connector_states().unwrap();

        if !states.is_empty() {
            println!("changed : {states:?}");
            std::io::stdout().flush().unwrap();
        }
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct ConnectorKey {
    card_hash: u64,
    connector_id: u32,
}

use std::hash::Hasher;
impl ConnectorKey {
    fn new(card_path: &str, connector_id: u32) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        card_path.hash(&mut hasher);
        Self {
            card_hash: hasher.finish(),
            connector_id,
        }
    }
}

impl std::fmt::Debug for ConnectorKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ConnKey({})", self.connector_id)
    }
}

#[derive(Debug)]
struct Card {
    path: String,
    file: std::fs::File,
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
            path: path.to_string(),
            file: options.open(path)?,
        })
    }

    fn get_all_connector_states(
        &mut self,
    ) -> Result<Vec<(ConnectorKey, connector::State)>, Box<dyn std::error::Error>> {
        let mut states: Vec<(ConnectorKey, connector::State)> = Vec::new();
        let resources = self.resource_handles()?;

        for &conn_handle in resources.connectors() {
            let connector = self.get_connector(conn_handle, false).unwrap();
            let key = ConnectorKey::new(&self.path, conn_handle.into());
            states.push((key, connector.state()));
        }
        Ok(states)
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

    fn get_all_cards_connector_states(
        &mut self,
    ) -> Result<Vec<(ConnectorKey, connector::State)>, Box<dyn std::error::Error>> {
        let mut states: Vec<(ConnectorKey, connector::State)> = Vec::new();
        for card in &mut self.0 {
            states.extend(card.get_all_connector_states()?);
        }
        Ok(states)
    }
}

#[allow(unused)]
struct DebounceTrackder {
    pending: HashMap<ConnectorKey, (connector::State, std::time::Instant)>,
    stable: HashMap<ConnectorKey, connector::State>,
}

#[allow(unused)]
impl DebounceTrackder {
    fn new() -> Self {
        Self {
            pending: HashMap::new(),
            stable: HashMap::new(),
        }
    }

    fn update(
        &mut self,
        new: Vec<(ConnectorKey, connector::State)>,
    ) -> Vec<(ConnectorKey, connector::State)> {
        let now = std::time::Instant::now();
        let debounce_duration = std::time::Duration::from_millis(DEBOUNCE_MS);

        // TODO: keep registry of all known states and return all states for connectors that
        // are not in registry as stable, thanks to that freshly connected monitor will instantly
        // be verified as stable device and already connected that may appear as disconnected for
        // less that debounce duration will be debounced.

        // for (key, state) in current {
        //     let stable_state = self.stable.get(&key);
        //
        //     if stable_state != Some(&state) {
        //         self.pending.insert(key, (state, now));
        //     } else {
        //         self.pending.remove(&key);
        //     }
        // }
        //
        // let mut ready: Vec<(u32, connector::State)> = Vec::new();
        // self.pending.retain(|key, (state, last_update)| {
        //     if now.duration_since(*last_update) >= debounce_duration {
        //         ready.push((*key, *state));
        //         self.stable.insert(*key, *state);
        //         false
        //     } else {
        //         true
        //     }
        // });
        // ready
        todo!()
    }
}

struct OutputManager {
    cards: Cards,
    connectors: HashMap<ConnectorKey, connector::State>,
    socket: udev::MonitorSocket,
    debouncer: DebounceTrackder,
}

impl OutputManager {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut cards = Cards::new().unwrap();

        let connectors: HashMap<ConnectorKey, connector::State> = cards
            .get_all_cards_connector_states()?
            .into_iter()
            .collect();

        let debouncer = DebounceTrackder::new();

        let monitor = udev::MonitorBuilder::new()?;
        let monitor = monitor.match_subsystem_devtype("drm", "drm_minor")?;
        let socket = monitor.listen()?;

        Ok(OutputManager {
            cards,
            connectors,
            socket,
            debouncer,
        })
    }

    fn had_drm_event(&mut self) -> bool {
        let mut had_event = false;
        for event in self.socket.iter().take(10) {
            let event_type = event.event_type();
            if event_type == udev::EventType::Add || event_type == udev::EventType::Change {
                had_event = true;
            }
        }
        had_event
    }

    fn update_connector_states(
        &mut self,
    ) -> Result<Vec<(ConnectorKey, connector::State)>, Box<dyn std::error::Error>> {
        if self.had_drm_event() {
            let mut changed_states: Vec<(ConnectorKey, connector::State)> = Vec::new();
            let new_states = self.cards.get_all_cards_connector_states()?;

            for (key, new_state) in &new_states {
                let old_state = self.connectors.get(key);
                match (old_state, new_state) {
                    (None, new_state) => changed_states.push((*key, *new_state)),
                    (Some(old_state), new_state) if old_state != new_state => {
                        changed_states.push((*key, *new_state))
                    }
                    _ => (),
                };
            }
            self.connectors = new_states.into_iter().collect();

            Ok(changed_states)
        } else {
            Ok(Vec::with_capacity(0))
        }
    }
}
