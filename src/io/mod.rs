use key::Key;

pub mod events;
pub mod key;
pub mod roon;

// For this dummy application we only need two IO event
#[derive(Debug, Clone)]
pub enum IoEvent {
    Initialize,
    Input(Key),
    Tick,
    BrowseTitle(String),
}
