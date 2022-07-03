use std::time::{SystemTime, UNIX_EPOCH};

use egui::style::Margin;
use egui::{
    menu, CentralPanel, Color32, DragValue, Frame, Grid, RichText, Rounding, ScrollArea, SidePanel,
    TopBottomPanel, Vec2,
};

use serde::{Deserialize, Serialize};

use crate::api::{self, fetch, Data, Request};

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct DashboardApp {
    pub ip: String,
    pub poll_rate: usize,
    pub heatmap_delta: f32,
    #[serde(skip)]
    pub last_poll: u128,
    #[serde(skip)]
    request: Option<Request>,
    #[serde(skip)]
    pub data: Option<Data>,
    #[serde(skip)]
    pub error: Option<api::Error>,
}

impl Default for DashboardApp {
    fn default() -> Self {
        Self {
            ip: "http://192.168.0.200".into(),
            poll_rate: 2000,
            heatmap_delta: 200.0,
            last_poll: 0,
            request: None,
            data: None,
            error: None,
        }
    }
}

impl DashboardApp {
    pub fn new(context: &eframe::CreationContext) -> Self {
        let mut style = (*context.egui_ctx.style()).clone();
        for (_, f) in style.text_styles.iter_mut() {
            f.size = (f.size * 1.2).round();
        }
        context.egui_ctx.set_style(style);

        context
            .storage
            .and_then(|s| eframe::get_value::<Self>(s, eframe::APP_KEY))
            .unwrap_or_default()
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
                ui.horizontal(|ui| {
                    ui.set_width(160.0);
                    ui.text_edit_singleline(&mut self.ip);
                });

                ui.label("poll rate");
                ui.add(
                    DragValue::new(&mut self.poll_rate)
                        .clamp_range(100..=10000)
                        .speed(10),
                );

                ui.label("heatmap delta");
                ui.add(
                    DragValue::new(&mut self.heatmap_delta)
                        .clamp_range(5.0..=800.0)
                        .speed(1.0),
                );

                if self.request.is_some() {
                    ui.spinner();
                }
            });
        });

        CentralPanel::default().show(ctx, |ui| {
            let panel_fill = if ui.style().visuals.dark_mode {
                Color32::from_gray(0x20)
            } else {
                Color32::from_gray(0xf0)
            };
            SidePanel::left("stats")
                .resizable(false)
                .frame(Frame {
                    inner_margin: Margin::same(6.0),
                    rounding: Rounding::same(5.0),
                    fill: panel_fill,
                    ..Default::default()
                })
                .show_inside(ui, |ui| {
                    if let Some(data) = &self.data {
                        ScrollArea::vertical().show(ui, |ui| {
                            Grid::new("stats_container").show(ui, |ui| {
                                ui.label("Voltage");
                                ui.label(data.main.voltage.to_string());
                                ui.label("V");
                                ui.end_row();

                                ui.label("Min cell voltage");
                                ui.label(data.ucell.min_voltage.to_string());
                                ui.label("mV");
                                ui.end_row();

                                ui.label("Avg cell voltage");
                                ui.label(data.ucell.avg_voltage.to_string());
                                ui.label("mV");
                                ui.end_row();

                                ui.label("Max cell voltage");
                                ui.label(data.ucell.max_voltage.to_string());
                                ui.label("mV");
                                ui.end_row();

                                let delta = data.ucell.max_voltage - data.ucell.min_voltage;
                                ui.label("Delta cell voltage");
                                ui.label(delta.to_string());
                                ui.label("mV");
                                ui.end_row();

                                ui.label("Current");
                                ui.label(data.main.current.to_string());
                                ui.label("mA");
                                ui.end_row();

                                ui.label("State of charge");
                                ui.label(data.main.state_of_charge.to_string());
                                ui.label("%");
                                ui.end_row();
                                ui.end_row();

                                ui.label("Min temperature");
                                ui.label(data.main.temp_min.to_string());
                                ui.label("°C");
                                ui.end_row();

                                ui.label("Avg temperature");
                                ui.label(data.main.temp_avg.to_string());
                                ui.label("°C");
                                ui.end_row();

                                ui.label("Max temperature");
                                ui.label(data.main.temp_max.to_string());
                                ui.label("°C");
                                ui.end_row();

                                ui.label("Master temperature");
                                ui.label(data.main.temp_master.to_string());
                                ui.label("°C");
                                ui.end_row();
                                ui.end_row();

                                ui.label("#Cells");
                                ui.label(data.ucell.num_cells.to_string());
                                ui.end_row();

                                ui.label("#Slaves");
                                ui.label(data.ucell.num_slaves.to_string());
                                ui.end_row();

                                ui.label("#Cells / #Slaves");
                                ui.label(data.ucell.num_cells_per_slave.to_string());
                                ui.end_row();

                                ui.label("#Temperature sensors");
                                ui.label(data.ucell.num_temp_sensors.to_string());
                                ui.end_row();

                                ui.label("#Safe resistors");
                                ui.label(data.ucell.num_safe_resistors.to_string());
                                ui.end_row();
                            });
                        });
                    }
                });

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
    fn poll_data(&mut self) {
        match &self.request {
            Some(r) => {
                if r.is_finished() {
                    let result = self.request.take().unwrap().join();
                    match result {
                        Ok(d) => {
                            self.data = Some(d);
                            self.error = None;
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            }
            None => {
                let now = now();

                if self.last_poll + (self.poll_rate as u128) < now {
                    self.request = Some(fetch(&self.ip));
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
