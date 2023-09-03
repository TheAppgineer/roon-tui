use crossterm::event::KeyEvent;
use roon_api::{browse, transport::{QueueItem, QueueChange, Zone, ZoneSeek, volume, Control}};
use serde::{Deserialize, Serialize};

pub mod events;
pub mod roon;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QueueMode {
    #[default] Manual = 0,
    RoonRadio = 1,
    RandomAlbum = 2,
    RandomTrack = 3,
}

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
    QueueModeNext,
    QueueModeCurrent(QueueMode),
    Zones(Vec<(String, String)>),
    ZoneSelect,
    ZoneSelected(String),
    ZoneChanged(Zone),
    ZoneRemoved(String),
    ZoneSeek(ZoneSeek),
    Mute(volume::Mute),
    ChangeVolume(i32),
    Control(Control),
    PauseOnTrackEndReq,
    PauseOnTrackEndActive(bool),
}
