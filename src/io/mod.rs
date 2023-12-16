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

#[derive(Clone, Debug)]
pub enum EndPoint {
    Zone(String),
    Output(String),
    Preset(String),
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
    QueueListLast(Option<QueueItem>),
    QueueSelected(u32),
    QueueClear,
    QueueModeNext,
    QueueModeAppend,
    QueueModeCurrent(QueueMode),
    Zones(Vec<(EndPoint, String)>),
    ZoneSelect,
    ZoneSelected(EndPoint),
    ZoneChanged(Zone),
    ZoneRemoved(String),
    ZoneSeek(ZoneSeek),
    ZoneGroupReq,
    ZoneGrouping(Option<Vec<(String, String, bool)>>),
    ZoneGrouped(Vec<String>),
    ZoneSavePreset(String, Vec<String>),
    ZoneMatchPreset(Vec<String>),
    ZonePresetMatched(Option<String>),
    Mute(volume::Mute),
    ChangeVolume(i32),
    Control(Control),
    Repeat,
    Shuffle,
    PauseOnTrackEndReq,
    PauseOnTrackEndActive(bool),
}
