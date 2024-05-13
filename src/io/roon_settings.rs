use roon_api::{
    RoonApi,
    settings::{BoxedSerTrait, Dropdown, Layout, SerTrait, Settings, Widget, Label},
    Svc,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::{EndPoint, QueueMode};

type Grouping = Vec<(String, Option<f32>)>;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueAction {
    PlayNow,
    AddNext,
    Queue,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Persistent {
    zone_id: Option<String>,
    profile: Option<String>,
    queue_modes: Option<HashMap<String, QueueMode>>,
    presets: Option<HashMap<String, Grouping>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ScratchPad {
    zone_id: Option<String>,
    profile: Option<String>,
    queue_mode: Option<QueueMode>,
    queue_action: Option<QueueAction>,
    dirty: bool,
}

#[derive(Debug, Default)]
pub struct RoonSettings {
    persistent: Arc<Mutex<Persistent>>,
    config_path: Arc<String>,
    zone_list: Arc<Mutex<Vec<(EndPoint, String)>>>,
}

#[derive(Deserialize, Serialize)]
struct QueueModeEntry {
    title: String,
    value: QueueMode,
}

#[typetag::serde]
impl SerTrait for QueueModeEntry {}

impl QueueModeEntry {
    fn from(title: &str, value: QueueMode) -> BoxedSerTrait {
        Box::new(
            Self {
                title: title.to_owned(),
                value,
            }
        ) as BoxedSerTrait
    }
}

#[derive(Deserialize, Serialize)]
struct QueueActionEntry {
    title: String,
    value: QueueAction,
}

#[typetag::serde]
impl SerTrait for QueueActionEntry {}

impl QueueActionEntry {
    fn from(title: &str, value: QueueAction) -> BoxedSerTrait {
        Box::new(
            Self {
                title: title.to_owned(),
                value,
            }
        ) as BoxedSerTrait
    }
}

#[derive(Deserialize, Serialize)]
struct EndPointEntry {
    title: String,
    value: String,
}

#[typetag::serde]
impl SerTrait for EndPointEntry {}

impl EndPointEntry {
    fn from(title: String, value: &str) -> BoxedSerTrait {
        Box::new(
            Self {
                title: title.to_owned(),
                value: value.to_owned(),
            }
        ) as BoxedSerTrait
    }
}

impl Persistent {
    fn new(value: Value) -> Self {
        let mut this: Self = serde_json::from_value(value).unwrap_or_default();

        if this.presets.is_none() {
            this.presets = Some(HashMap::new());
        }

        if this.queue_modes.is_none() {
            this.queue_modes = Some(HashMap::new())
        }

        this
    }
}

impl ScratchPad {
    fn from(persistent: &Persistent) -> Self {
        let zone_id = persistent.zone_id.to_owned();
        let profile = persistent.profile.to_owned();
        let queue_mode = zone_id.as_deref()
            .and_then(|zone_id| {
                persistent.queue_modes.as_ref()
                    .map(|queue_modes| {
                        queue_modes.get(zone_id).cloned().unwrap_or_default()
                    })
            });

        ScratchPad {
            zone_id,
            profile,
            queue_mode,
            queue_action: None,
            dirty: false,
        }
    }
}

impl RoonSettings {
    pub fn new(roon: &RoonApi, config_path: Arc<String>) -> (Svc, Settings, RoonSettings) {
        let value = RoonApi::load_config(&config_path, "settings");
        let persistent = Arc::new(Mutex::new(Persistent::new(value)));
        let config_path_clone = config_path.clone();
        let zone_list = Arc::new(Mutex::new(Vec::new()));

        let zone_list_clone = zone_list.clone();
        let persistent_clone = persistent.clone();
        let get_layout_cb = move |settings: Option<ScratchPad>| -> Layout<ScratchPad> {
            let zone_list = zone_list_clone.lock().unwrap();
            let settings = match settings {
                Some(mut settings) => {
                    let persistent = persistent_clone.lock().unwrap();
                    Self::sync(&persistent, &mut settings);
                    settings
                },
                None => {
                    let value = RoonApi::load_config(&config_path_clone, "settings");
                    let persistent = Persistent::new(value);

                    ScratchPad::from(&persistent)
                }
            };

            RoonSettings::make_layout(settings, &zone_list)
        };
        let (svc, settings) = Settings::new(
            roon,
            Box::new(get_layout_cb),
        );
        let this = RoonSettings {
            config_path,
            persistent,
            zone_list,
        };

        (svc, settings, this)
    }

    pub fn save(&self) {
        let settings = self.persistent.serialize(serde_json::value::Serializer).unwrap();

        RoonApi::save_config(&self.config_path, "settings", settings).unwrap();
    }

    pub fn update(&mut self, settings: serde_json::Value) -> Option<(EndPoint, Option<QueueAction>)> {
        let scratch_pad = serde_json::from_value::<ScratchPad>(settings).unwrap();
        let mut persistent = self.persistent.lock().unwrap();

        persistent.zone_id = scratch_pad.zone_id.to_owned();

        let end_point = scratch_pad.zone_id.map(|end_point_id| {
            match end_point_id.chars().nth(1) {
                Some('6') => EndPoint::Zone(end_point_id),
                Some('7') => EndPoint::Output(end_point_id),
                _ => EndPoint::Preset(end_point_id),
            }
        })?;
        let zone_id = persistent.zone_id.to_owned()?;

        persistent.queue_modes.as_mut()?.insert(zone_id, scratch_pad.queue_mode?);

        Some((end_point, scratch_pad.queue_action))
    }

    pub fn set_zone_list(&mut self, zone_list: &Vec<(EndPoint, String)>) {
        let mut zones = self.zone_list.lock().unwrap();

        *zones = zone_list.to_owned();
    }

    pub fn add_preset(&mut self, name: String, preset: Grouping) -> Option<()> {
        let mut persistent = self.persistent.lock().unwrap();
        let presets = persistent.presets.as_mut()?;

        presets.insert(name, preset);

        Some(())
    }

    pub fn remove_preset(&mut self, name: &str) -> Option<Grouping> {
        let mut persistent = self.persistent.lock().unwrap();
        let presets = persistent.presets.as_mut()?;

        presets.remove(name)
    }

    pub fn get_preset_names(&self) -> Option<Vec<String>> {
        let persistent = self.persistent.lock().unwrap();

        Some(persistent.presets.as_ref()?.keys()
            .map(|name| name.to_owned())
            .collect::<Vec<_>>())
    }

    pub fn get_preset(&self, name: &str) -> Option<Grouping> {
        let persistent = self.persistent.lock().unwrap();

        persistent.presets.as_ref()?.get(name).cloned()
    }

    pub fn match_preset(&self, output_ids: &mut Vec<String>) -> Option<String> {
        let persistent = self.persistent.lock().unwrap();

        output_ids[1..].sort();

        persistent.presets.as_ref()?.iter()
            .find_map(|(preset, preset_output_ids)| {
                if output_ids.len() == preset_output_ids.len() {
                    let matches = preset_output_ids.iter()
                        .filter(|(preset_output_id, _)| {
                            output_ids.contains(preset_output_id)
                        })
                        .count() == preset_output_ids.len();

                    if matches {
                        Some(preset.to_owned())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
    }

    pub fn get_queue_mode(&self) -> Option<QueueMode> {
        let persistent = self.persistent.lock().unwrap();
        let queue_modes = persistent.queue_modes.as_ref()?;
        let zone_id = persistent.zone_id.as_deref()?;

        Some(queue_modes.get(zone_id).cloned().unwrap_or_default())
    }

    pub fn set_queue_mode(&mut self, zone_or_output_id: &str, queue_mode: QueueMode) -> Option<()> {
        let mut persistent = self.persistent.lock().unwrap();
        let queue_modes = persistent.queue_modes.as_mut()?;

        queue_modes.insert(zone_or_output_id.to_owned(), queue_mode);

        Some(())
    }

    pub fn remove_queue_mode(&self, zone_or_output_id: &str) -> Option<QueueMode> {
        let mut persistent = self.persistent.lock().unwrap();

        persistent.queue_modes.as_mut()?.remove(zone_or_output_id)
    }

    pub fn get_zone_id(&self) -> Option<String> {
        let persistent = &self.persistent.lock().unwrap();

        persistent.zone_id.to_owned()
    }

    pub fn set_zone_id(&mut self, zone_id: Option<String>) {
        let mut persistent = self.persistent.lock().unwrap();

        persistent.zone_id = zone_id;
    }

    pub fn get_profile(&self) -> Option<String> {
        let persistent = &self.persistent.lock().unwrap();

        persistent.profile.to_owned()
    }

    pub fn set_profile(&mut self, profile: Option<String>) {
        let mut persistent = self.persistent.lock().unwrap();

        persistent.profile = profile;
    }

    fn sync(persistent: &Persistent, settings: &mut ScratchPad) -> Option<()> {
        if !settings.dirty && settings.zone_id != persistent.zone_id {
            let zone_id = settings.zone_id.as_deref()?;
            let queue_mode = persistent.queue_modes.as_ref()?.get(zone_id);

            settings.queue_mode = Some(queue_mode.cloned().unwrap_or_default());
            settings.dirty = true;
        }

        Some(())
    }

    fn make_layout(settings: ScratchPad, zone_list: &[(EndPoint, String)]) -> Layout<ScratchPad> {
        let end_points = zone_list.iter()
            .map(|(end_point, name)| {
                match end_point {
                    EndPoint::Zone(zone_id) => {
                        EndPointEntry::from(name.to_owned(), zone_id)
                    }
                    EndPoint::Output(output_id) => {
                        EndPointEntry::from(format!("<{}>", name), output_id)
                    }
                    EndPoint::Preset(preset) => {
                        EndPointEntry::from(format!("[{}]", name), preset)
                    }
                    EndPoint::MatchedPreset((zone_id, preset)) => {
                        EndPointEntry::from(preset.to_owned(), zone_id)
                    }
                }
            })
            .collect::<Vec<_>>();
        let zone_list_widget = Widget::Dropdown(Dropdown {
            title: "Zones",
            subtitle: Some("The available zones, <outputs> and [presets]".to_owned()),
            values: end_points,
            setting: "zone_id",
        });
        let mut widgets = Vec::new();

        if let Some(set_zone_id) = settings.zone_id.as_deref() {
            let zone_name = zone_list.iter()
                .find_map(|(end_point, name)| {
                    match end_point {
                        EndPoint::Zone(zone_id) => {
                            if zone_id == set_zone_id {
                                Some((name.as_str(), None))
                            } else {
                                None
                            }
                        }
                        EndPoint::MatchedPreset((zone_id, preset)) => {
                            if zone_id == set_zone_id {
                                Some((preset.as_str(), Some(name.as_str())))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                });
            let queue_modes = if settings.profile.is_none() {
                vec![
                    QueueModeEntry::from("Manual", QueueMode::Manual),
                    QueueModeEntry::from("Roon Radio", QueueMode::RoonRadio),
                ]
            } else {
                vec![
                    QueueModeEntry::from("Manual", QueueMode::Manual),
                    QueueModeEntry::from("Roon Radio", QueueMode::RoonRadio),
                    QueueModeEntry::from("Random Album", QueueMode::RandomAlbum),
                    QueueModeEntry::from("Random Track", QueueMode::RandomTrack),
                ]
            };
            let subtitle = if settings.profile.is_none() {
                Some("Select a profile in Roon TUI Browse View for additional modes".to_owned())
            } else {
                None
            };
            let queue_mode_widget = Widget::Dropdown(Dropdown {
                title: "Queue Mode",
                subtitle,
                values: queue_modes,
                setting: "queue_mode",
            });

            if let Some((zone_name, group_name)) = zone_name {
                let subtitle = group_name.map(|group_name| {
                    format!("The group name in Roon is \"{}\"", group_name)
                });
                let zone_name_widget = Widget::Label(Label {
                    title: format!("The currently controlled zone is {}", zone_name),
                    subtitle,
                });

                widgets.push(zone_name_widget);
            }

            widgets.push(zone_list_widget);
            widgets.push(queue_mode_widget);

            if settings.profile.is_some() {
                match settings.queue_mode {
                    Some(QueueMode::RandomAlbum) | Some(QueueMode::RandomTrack) => {
                        let queue_actions = vec![
                            QueueActionEntry::from("Play Now", QueueAction::PlayNow),
                            QueueActionEntry::from("Add Next", QueueAction::AddNext),
                            QueueActionEntry::from("Queue", QueueAction::Queue),
                        ];
                        let queue_action_widget = Widget::Dropdown(Dropdown {
                            title: "Queue Action",
                            subtitle: None,
                            values: queue_actions,
                            setting: "queue_action",
                        });

                        widgets.push(queue_action_widget);
                    }
                    _ => (),
                }
            }
        } else {
            widgets.push(zone_list_widget);
        };

        Layout {
            settings,
            widgets,
            has_error: false,
        }
    }
}
