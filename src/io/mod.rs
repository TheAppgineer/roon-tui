use crossterm::event::KeyEvent;
use rust_roon_api::browse;

pub mod events;
pub mod roon;

// For this dummy application we only need two IO event
#[derive(Debug)]
pub enum IoEvent {
    Input(KeyEvent),
    Tick,
    CoreName(Option<String>),
    BrowseTitle(String),
    BrowseList(Vec<browse::Item>),
    BrowseAppend(Vec<browse::Item>),
    BrowseSelected(Option<String>),
    BrowseBack,
    BrowseHome,
}
