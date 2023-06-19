use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rfd::FileDialog;
use rustube::{tokio::sync::watch, Callback, Id, Video};
use serde::{Deserialize, Serialize};
use strum::EnumIter;
use tokio::sync::watch::Sender;

async fn download<'a>(url: String, path: &PathBuf, tx: Sender<f32>) {
    let id = Id::from_raw(&url).unwrap();

    let callback = Callback::new().connect_on_progress_closure(move |x| {
        let percentage = x.current_chunk as f32 / x.content_length.unwrap() as f32;
        tx.send(percentage).expect("Failed to send");
        // progress(percentage, 20)
    });

    Video::from_id(id.into_owned())
        .await
        .unwrap()
        .best_audio()
        .unwrap()
        .download_to_with_callback(path, callback)
        .await
        .unwrap();
}

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(Deserialize, Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct App {
    url: String,
    download_type: DownloadType,

    #[serde(skip)]
    state: Arc<Mutex<AppState>>,

    #[serde(skip)]
    value: Arc<Mutex<f32>>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            url: "".to_owned(),
            download_type: DownloadType::AudioOnly,
            value: Arc::new(Mutex::new(0.0)),
            state: Arc::new(Mutex::new(AppState::Initial)),
        }
    }
}

impl App {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        Default::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, EnumIter, Deserialize, Serialize)]
pub enum DownloadType {
    #[default]
    AudioOnly,
    VideoAudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, EnumIter, Deserialize, Serialize)]
pub enum AppState {
    #[default]
    Initial,
    Downloading,
    Done,
}

impl ToString for DownloadType {
    fn to_string(&self) -> String {
        let string = match self {
            Self::AudioOnly => "Audio Only",
            Self::VideoAudio => "Video + Audio",
        };

        string.to_string()
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let Self {
            url, value, state, ..
        } = self;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Youtube URL");
            let state_val: AppState;
            {
                state_val = state.lock().unwrap().clone();
            }

            if ui
                .add_enabled(
                    !matches!(state_val, AppState::Downloading),
                    egui::TextEdit::singleline(url),
                )
                .changed()
            {
                *state.lock().unwrap() = AppState::Initial;
            }

            // ui.add_enabled_ui(!matches!(state_val, AppState::Downloading), |ui| {
            //     egui::ComboBox::from_label("label")
            //         .selected_text(self.download_type.to_string())
            //         .show_ui(ui, |ui| {
            //             DownloadType::iter().for_each(|t| {
            //                 if ui
            //                     .selectable_value(&mut self.download_type, t, t.to_string())
            //                     .clicked()
            //                 {
            //                     *state.lock().unwrap() = AppState::Initial;
            //                 }
            //             });
            //         })
            // });

            if ui
                .add_enabled(
                    !matches!(state_val, AppState::Downloading),
                    egui::Button::new("Download"),
                )
                .clicked()
            {
                let file = FileDialog::new()
                    .add_filter("mp3", &["mp3"])
                    .set_file_name("soundtrack.mp3")
                    .save_file();

                if let Some(path) = file {
                    {
                        *state.lock().unwrap() = AppState::Downloading;
                    }

                    let (tx, mut rx) = watch::channel(0.0);
                    let value_arc = Arc::clone(value);
                    let ctx = ctx.clone();
                    let url = url.clone();

                    let state_mutex = Arc::clone(state);

                    // Download thread
                    tokio::spawn(async move {
                        download(url, &path, tx).await;
                        *state_mutex.lock().unwrap() = AppState::Done;
                    });

                    // Handle callback thread
                    tokio::spawn(async move {
                        while rx.changed().await.is_ok() {
                            *value_arc.lock().unwrap() = *rx.borrow();
                            ctx.request_repaint();
                        }
                    });
                }
            }

            match state_val {
                AppState::Done => {
                    ui.label("Download complete!");
                }
                AppState::Initial => (),
                AppState::Downloading => {
                    ui.label("Downloading...");
                    ui.add(
                        egui::ProgressBar::new(*value.lock().unwrap())
                            .animate(true)
                            .show_percentage(),
                    );
                }
            };

            egui::warn_if_debug_build(ui);
        });
    }
}
