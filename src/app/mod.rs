use log::{debug, warn};
use tokio::sync::mpsc;

use self::actions::Actions;
use self::state::AppState;
use crate::app::actions::Action;
use crate::io::{IoEvent, key::Key};

pub mod actions;
pub mod state;
pub mod ui;

#[derive(Debug, PartialEq, Eq)]
pub enum AppReturn {
    Exit,
    Continue,
}

/// The main application, containing the state
pub struct App {
    io_rx: mpsc::Receiver<IoEvent>,
    actions: Actions,
    is_loading: bool,
    state: AppState,
    browse_title: String,
}

impl App {
    pub fn new(io_rx: mpsc::Receiver<IoEvent>) -> Self {
        let actions = vec![Action::Quit].into();
        let is_loading = false;
        let state = AppState::default();

        Self {
            io_rx,
            actions,
            is_loading,
            state,
            browse_title: "Browse".to_owned(),
        }
    }

    /// Handle a user action
    pub async fn do_action(&mut self, key: Key) -> AppReturn {
        if let Some(action) = self.actions.find(key) {
            debug!("Run action [{:?}]", action);
            match action {
                Action::Quit => AppReturn::Exit,
            }
        } else {
            warn!("No action accociated to {}", key);
            AppReturn::Continue
        }
    }

    /// We could update the app or dispatch event on tick
    pub async fn update_on_tick(&mut self) -> AppReturn {
        // here we just increment a counter
        self.state.incr_tick();
        AppReturn::Continue
    }

    pub async fn update_on_event(&mut self) -> AppReturn {
        // `is_loading` will be set to false again after the async action has finished in io/handler.rs
        self.is_loading = true;

        if let Some(io_event) = self.io_rx.recv().await {
            match io_event {
                IoEvent::Initialize => self.initialized(),
                IoEvent::Input(key) => self.do_action(key).await,
                IoEvent::Tick => self.update_on_tick().await,
                IoEvent::BrowseTitle(browse_title) => {
                    self.browse_title = browse_title;

                    AppReturn::Continue
                }
            }
        } else {
            AppReturn::Continue
        }
    }

    pub fn actions(&self) -> &Actions {
        &self.actions
    }
    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    fn initialized(&mut self) -> AppReturn {
        // Update contextual actions
        self.actions = vec![Action::Quit].into();
        self.state = AppState::initialized();

        AppReturn::Continue
    }

    pub fn loaded(&mut self) {
        self.is_loading = false;
    }

    pub fn sleeped(&mut self) {
        self.state.incr_sleep();
    }
}
