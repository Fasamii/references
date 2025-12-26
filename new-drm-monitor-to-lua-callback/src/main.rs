use drm::control::{Device, connector};
use mlua::Chunk;
use std::{collections::HashMap, hash::Hash};

const CONFIG_PATH: &str = "config.lua";

fn main() {
    let mut core = Core::new(CONFIG_PATH).unwrap();
    println!("Inital State : {:?}", core.output.connectors);
    loop {
        core.dispatch().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
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
            println!("conn = {}", connector);
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

struct OutputManager {
    cards: Cards,
    connectors: HashMap<ConnectorKey, connector::State>,
    socket: udev::MonitorSocket,
}

impl OutputManager {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut cards = Cards::new().unwrap();

        let connectors: HashMap<ConnectorKey, connector::State> = cards
            .get_all_cards_connector_states()?
            .into_iter()
            .collect();

        let monitor = udev::MonitorBuilder::new()?;
        let monitor = monitor.match_subsystem_devtype("drm", "drm_minor")?;
        let socket = monitor.listen()?;

        Ok(OutputManager {
            cards,
            connectors,
            socket,
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
        let mut changed_states: Vec<(ConnectorKey, connector::State)> = Vec::new();
        if self.had_drm_event() {
            let all_states = self.cards.get_all_cards_connector_states()?;
            for (key, current_state) in &all_states {
                let old_state = self.connectors.get(key);
                match (old_state, current_state) {
                    (None, current_state) => changed_states.push((*key, *current_state)),
                    (Some(old_state), current_state) if old_state != current_state => {
                        changed_states.push((*key, *current_state))
                    }
                    _ => (),
                };
            }
            self.connectors = all_states.into_iter().collect();
            Ok(changed_states)
        } else {
            Ok(Vec::with_capacity(0))
        }
    }
}

struct Config<'a> {
    lua: mlua::Lua,
    config: Chunk<'a>,
}

impl<'a> Config<'a> {
    fn new(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let lua = mlua::Lua::new();
        let config = lua.load(Self::read_config_file(path)?);
        Ok(Self { lua, config })
    }

    fn read_config_file(path: &str) -> Result<String, std::io::Error> {
        std::fs::read_to_string(path)
    }
}

struct Core<'a> {
    output: OutputManager,
    config: Config<'a>,
}

impl<'a> Core<'a> {
    fn new(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            output: OutputManager::new()?,
            config: Config::new(path)?,
        })
    }

    fn dispatch(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let changed_states = self.output.update_connector_states()?;
        changed_states
            .iter()
            .for_each(|state| println!("Changed_state: {state:?}"));

        Ok(())
    }
}
