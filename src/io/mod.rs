use crossterm::event::KeyEvent;
use rust_roon_api::{browse, transport::{QueueItem, QueueChange}};

pub mod events;
pub mod roon;

#[derive(Debug)]
pub enum IoEvent {
    Input(KeyEvent),
    Tick,
    CoreName(Option<String>),
    BrowseTitle(String),
    BrowseList(usize, Vec<browse::Item>),
    BrowseSelected((Option<String>, Option<String>)),
    BrowseBack,
    BrowseRefresh,
    BrowseHome,
    QueueList(Vec<QueueItem>),
    QueueListChanges(Vec<QueueChange>),
    QueueSelected(u32),
    Zones(Vec<(String, String)>),
    ZoneSelect,
    ZoneSelected(String),
}
