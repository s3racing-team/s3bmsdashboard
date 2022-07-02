use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

use egui::{
    menu, CentralPanel, Color32, DragValue, FontFamily, FontId, Grid, RichText, TopBottomPanel,
    Vec2,
};

use serde::{Deserialize, Serialize};

use crate::api::{self, Data};

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct DashboardApp {
    pub ip: String,
    pub poll_rate: usize,
    pub heatmap_delta: f32,
    #[serde(skip)]
    pub last_poll: u128,
    #[serde(skip)]
    task: Option<JoinHandle<anyhow::Result<Data>>>,
    #[serde(skip)]
    pub data: Option<Data>,
    #[serde(skip)]
    pub error: Option<api::Error>,
}

impl Default for DashboardApp {
    fn default() -> Self {
        Self {
            ip: "192.168.0.200".into(),
            poll_rate: 2000,
            heatmap_delta: 200.0,
            last_poll: 0,
            task: None,
            data: None,
            error: None,
        }
    }
}

impl eframe::App for DashboardApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll_data();

        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            menu::bar(ui, |ui| {
                ui.label("ip");
                ui.text_edit_singleline(&mut self.ip);
                ui.label("poll rate");
                ui.add(
                    DragValue::new(&mut self.poll_rate)
                        .clamp_range(100..=10000)
                        .speed(10),
                );
                ui.label("heatmap delta");
                ui.add(
                    DragValue::new(&mut self.heatmap_delta)
                        .clamp_range(20.0..=500.0)
                        .speed(10),
                );

                if self.task.is_some() {
                    ui.spinner();
                }
            });
        });

        CentralPanel::default().show(ctx, |ui| {
            if let Some(data) = &self.data {
                let spacing = 10.0;
                let w = (ui.available_width() - spacing * 11.0) / 10.0;
                Grid::new("cells")
                    .min_col_width(w)
                    .max_col_width(w)
                    .spacing(Vec2::new(spacing, spacing))
                    .show(ui, |ui| {
                        let ucell = &data.ucell;

                        for (i, row) in ucell.cell_voltage.chunks(9).enumerate() {
                            ui.label(RichText::new((i + 1).to_string()).monospace());
                            for &cell in row {
                                let bg = heatmap_color(ucell.avg_voltage, cell, self.heatmap_delta);
                                ui.label(
                                    RichText::new(cell.to_string())
                                        .background_color(bg)
                                        .font(FontId::new(24.0, FontFamily::Monospace))
                                        .monospace(),
                                );
                            }
                            ui.end_row();
                        }
                    });
            }
            match &self.error {
                Some(api::Error::Fetch(e)) => {
                    let text = format!("Could not fetch data:\n {e}");
                    ui.horizontal_centered(|ui| {
                        ui.label(RichText::new(&text).color(Color32::RED));
                    });
                }
                Some(api::Error::Unexpected) => {
                    ui.horizontal_centered(|ui| {
                        ui.label(RichText::new("Unexpected error").color(Color32::RED));
                    });
                }
                None => (),
            }
        });
    }
}

impl DashboardApp {
    pub fn new(context: &eframe::CreationContext) -> Self {
        context
            .storage
            .and_then(|s| eframe::get_value::<Self>(s, eframe::APP_KEY))
            .unwrap_or_default()
    }

    fn poll_data(&mut self) {
        match &self.task {
            Some(t) => {
                if t.is_finished() {
                    let result = self.task.take().unwrap().join();
                    match result {
                        Ok(Ok(d)) => {
                            self.data = Some(d);
                            self.error = None;
                        }
                        Ok(Err(e)) => self.error = Some(api::Error::Fetch(e)),
                        Err(_) => self.error = Some(api::Error::Unexpected),
                    }
                }
            }
            None => {
                let now = now();

                if self.last_poll + (self.poll_rate as u128) < now {
                    let ip = self.ip.clone();
                    self.task = Some(thread::spawn(move || api::fetch(&ip)));
                    self.last_poll = now;
                }
            }
        }
    }
}

fn heatmap_color(avg: u16, cell: u16, delta: f32) -> Color32 {
    let diff = ((cell as f32 - avg as f32) / delta).clamp(-1.0, 1.0);
    if diff < 0.0 {
        let r = (-255.0 * diff) as u8;
        Color32::from_rgb(r, 30, 30)
    } else {
        let b = (255.0 * diff) as u8;
        Color32::from_rgb(30, 30, b)
    }
}

fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
