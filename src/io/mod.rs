use crossterm::event::KeyEvent;
use roon_api::{browse, transport::{QueueItem, QueueChange, Zone, ZoneSeek, volume, Control}};

pub mod events;
pub mod roon;

#[derive(Debug)]
pub enum IoEvent {
    Input(KeyEvent),
    Redraw,
    CoreName(Option<String>),
    BrowseTitle(String),
    BrowseList(usize, Vec<browse::Item>),
    BrowseSelected(Option<String>),
    BrowseBack,
    BrowseRefresh,
    BrowseHome,
    BrowseInput(String),
    QueueList(Vec<QueueItem>),
    QueueListChanges(Vec<QueueChange>),
    QueueSelected(u32),
    Zones(Vec<(String, String)>),
    ZoneSelect,
    ZoneSelected(String),
    ZoneChanged(Zone),
    ZoneRemoved(String),
    ZoneSeek(ZoneSeek),
    Mute(volume::Mute),
    ChangeVolume(i32),
    Control(Control),
}
