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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Persistent {
    zone_id: Option<String>,
    prim_output_id: Option<String>,
    profile: Option<String>,
    queue_modes: Option<HashMap<String, QueueMode>>,
    presets: Option<HashMap<String, Grouping>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ScratchPad {
    zone_id: Option<String>,
    profile: Option<String>,
    queue_mode: QueueMode,
}

#[derive(Debug, Default)]
pub struct RoonSettings {
    pub persistent: Persistent,
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
    fn new(title: &str, value: QueueMode) -> BoxedSerTrait {
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
    fn new(title: String, value: &str) -> BoxedSerTrait {
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

    pub fn get_zone_id(&self) -> Option<&str> {
        self.zone_id.as_deref()
    }

    pub fn set_zone_id(&mut self, zone_id: Option<String>) {
        self.zone_id = zone_id;
    }

    pub fn set_prim_output_id(&mut self, prim_output_id: Option<&str>) {
        self.prim_output_id = prim_output_id.map(|output_id| output_id.to_owned());
    }

    pub fn get_profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }

    pub fn set_profile(&mut self, profile: Option<String>) {
        self.profile = profile;
    }

    pub fn get_presets(&self) -> Option<&HashMap<String, Grouping>> {
        self.presets.as_ref()
    }

    pub fn get_presets_mut(&mut self) -> Option<&mut HashMap<String, Grouping>> {
        self.presets.as_mut()
    }

    pub fn get_queue_modes(&self) -> Option<&HashMap<String, QueueMode>> {
        self.queue_modes.as_ref()
    }

    pub fn get_queue_modes_mut(&mut self) -> Option<&mut HashMap<String, QueueMode>> {
        self.queue_modes.as_mut()
    }
}

impl ScratchPad {
    fn from(persistent: &Persistent) -> Self {
        let zone_id = persistent.zone_id.to_owned();
        let profile = persistent.profile.to_owned();
        let queue_mode = if let Some(prim_output_id) = persistent.prim_output_id.as_deref() {
            if let Some(queue_mode) = persistent.get_queue_modes() {
                queue_mode.get(prim_output_id).cloned().unwrap_or_default()
            } else {
                QueueMode::Manual
            }
        } else {
            QueueMode::Manual
        };

        ScratchPad {
            zone_id,
            profile,
            queue_mode,
        }
    }
}

impl RoonSettings {
    pub fn new(roon: &RoonApi, config_path: Arc<String>) -> (Svc, Settings, RoonSettings) {
        let value = RoonApi::load_config(&config_path, "settings");
        let config_path_clone = config_path.clone();
        let zone_list = Arc::new(Mutex::new(Vec::new()));

        let zone_list_clone = zone_list.clone();
        let get_layout_cb = move |settings: Option<ScratchPad>| -> Layout<ScratchPad> {
            let zone_list = zone_list_clone.lock().unwrap();
            let settings = match settings {
                Some(settings) => settings,
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
            persistent: Persistent::new(value),
            zone_list,
        };

        (svc, settings, this)
    }

    pub fn save(&self) {
        let settings = self.persistent.serialize(serde_json::value::Serializer).unwrap();

        RoonApi::save_config(&self.config_path, "settings", settings).unwrap();
    }

    pub fn update(&mut self, settings: serde_json::Value) -> (Option<EndPoint>, QueueMode) {
        let scratch_pad = serde_json::from_value::<ScratchPad>(settings).unwrap();
        let end_point = scratch_pad.zone_id.map(|end_point_id| {
            match end_point_id.chars().nth(1) {
                Some('6') => EndPoint::Zone(end_point_id),
                Some('7') => EndPoint::Output(end_point_id),
                _ => EndPoint::Preset(end_point_id),
            }
        });

        if let Some(queue_modes) = self.persistent.queue_modes.as_mut() {
            if let Some(output_id) = self.persistent.prim_output_id.as_ref() {
                queue_modes.insert(output_id.to_owned(), scratch_pad.queue_mode.to_owned());
            }
        }

        (end_point, scratch_pad.queue_mode)
    }

    pub fn set_zone_list(&mut self, zone_list: &Vec<(EndPoint, String)>) {
        let mut zones = self.zone_list.lock().unwrap();

        *zones = zone_list.to_owned();
    }

    fn make_layout(settings: ScratchPad, zone_list: &Vec<(EndPoint, String)>) -> Layout<ScratchPad> {
        let end_points = zone_list.iter()
            .map(|(end_point, name)| {
                match end_point {
                    EndPoint::Zone(zone_id) => {
                        EndPointEntry::new(format!("{}", name), zone_id)
                    }
                    EndPoint::Output(output_id) => {
                        EndPointEntry::new(format!("<{}>", name), output_id)
                    }
                    EndPoint::Preset(preset) => {
                        EndPointEntry::new(format!("[{}]", name), preset)
                    }
                }
            })
            .collect::<Vec<_>>();
        let zone_list_widget = Widget::Dropdown(Dropdown {
            title: "Zones",
            subtitle: Some(format!("The available zones, <outputs> and [presets]")),
            values: end_points,
            setting: "zone_id",
        });

        let widgets = if let Some(set_zone_id) = settings.zone_id.as_deref() {
            let zone_name = zone_list.iter()
                .find_map(|(end_point, name)| {
                    match end_point {
                        EndPoint::Zone(zone_id) => {
                            if zone_id == set_zone_id {
                                Some(name.as_str())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                });
            let queue_modes = if settings.profile.is_none() {
                vec![
                    QueueModeEntry::new("Manual", QueueMode::Manual),
                    QueueModeEntry::new("Roon Radio", QueueMode::RoonRadio),
                ]
            } else {
                vec![
                    QueueModeEntry::new("Manual", QueueMode::Manual),
                    QueueModeEntry::new("Roon Radio", QueueMode::RoonRadio),
                    QueueModeEntry::new("Random Album", QueueMode::RandomAlbum),
                    QueueModeEntry::new("Random Track", QueueMode::RandomTrack),
                ]
            };
            let queue_mode_widget = Widget::Dropdown(Dropdown {
                title: "Queue Mode",
                subtitle: None,
                values: queue_modes,
                setting: "queue_mode",
            });

            if let Some(zone_name) = zone_name {
                let zone_name_widget = Widget::Label(Label {
                    title: format!("The currently controlled zone is {}", zone_name),
                    subtitle: None,
                });

                vec![
                    zone_name_widget,
                    zone_list_widget,
                    queue_mode_widget,
                ]
            } else {
                vec![
                    zone_list_widget,
                    queue_mode_widget,
                ]
            }
        } else {
            vec![
                zone_list_widget,
            ]
        };

        Layout {
            settings,
            widgets,
            has_error: false,
        }
    }
}
