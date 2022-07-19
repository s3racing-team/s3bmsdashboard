use std::time::{Duration, SystemTime, UNIX_EPOCH};

use egui::style::Margin;
use egui::{
    menu, CentralPanel, Color32, DragValue, FontFamily, FontId, Frame, Grid, Layout, Rect,
    RichText, Rounding, ScrollArea, SidePanel, TopBottomPanel, Ui, Vec2,
};

use serde::{Deserialize, Serialize};

use crate::api::{self, fetch, Data, Request, Ucell};

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct DashboardApp {
    pub safe: bool,
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
            safe: true,
            ip: "http://192.168.0.200".into(),
            poll_rate: 1000,
            heatmap_delta: 100.0,
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
        if ctx.input().key_down(egui::Key::V) && ctx.input().key_down(egui::Key::W) {
            self.safe = !self.safe;
        }

        self.poll_data();
        ctx.request_repaint_after(Duration::from_millis(100));

        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            menu::bar(ui, |ui| {
                if self.safe {
                    ui.label("s3racing");
                }
                ui.label("IP");
                ui.horizontal(|ui| {
                    ui.set_width(160.0);
                    ui.text_edit_singleline(&mut self.ip);
                });

                ui.label("Poll rate");
                ui.add(
                    DragValue::new(&mut self.poll_rate)
                        .clamp_range(100..=10000)
                        .speed(10),
                );

                ui.label("Heatmap delta");
                ui.add(
                    DragValue::new(&mut self.heatmap_delta)
                        .clamp_range(5.0..=1000.0)
                        .speed(1.0),
                );

                ui.with_layout(Layout::right_to_left(), |ui| {
                    if self.request.is_some() {
                        ui.spinner();
                        if ui.button("cancel").clicked() {
                            self.request = None;
                        }
                    }
                });
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

                                ui.label("#Slaves");
                                ui.label(data.ucell.num_slaves.to_string());
                                ui.end_row();

                                ui.label("#Cells");
                                ui.label(data.ucell.num_cells.to_string());
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

            match &self.error {
                Some(api::Error::Fetch(_)) => {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Error loading data").color(Color32::RED));
                    });
                }
                Some(api::Error::Unexpected) => {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Unexpected error").color(Color32::RED));
                    });
                }
                None => (),
            }

            if let Some(data) = &self.data {
                let pos = ui.cursor().min;
                let size = ui.available_size();
                let stack_size = size / Vec2::new(4.0, 2.0);

                const STACK_POS: [(f32, f32); 8] = [
                    (2.0, 1.0),
                    (2.0, 0.0),
                    (3.0, 0.0),
                    (3.0, 1.0),
                    (0.0, 1.0),
                    (0.0, 0.0),
                    (1.0, 0.0),
                    (1.0, 1.0),
                ];

                for (i, (x, y)) in STACK_POS.iter().enumerate() {
                    let stack_pos = pos + Vec2::new(x * stack_size.x, y * stack_size.y);
                    let stack_rect = Rect::from_min_size(stack_pos, stack_size);
                    let offset = i * 18;
                    ui.allocate_ui_at_rect(stack_rect, |ui| {
                        draw_stack(ui, &data.ucell, offset, self.heatmap_delta)
                    });
                }
            }
        });
    }
}

fn draw_stack(ui: &mut Ui, ucell: &Ucell, offset: usize, heatmap_delta: f32) {
    let pos = ui.cursor().min;
    let cell_size = ui.available_size() / Vec2::new(2.0, 9.0);

    for i in 0..9 {
        let cell_index = offset + (8 - i);
        let cell_voltage = ucell
            .cell_voltage
            .get(cell_index)
            .copied()
            .unwrap_or(u16::MAX);
        let bg_color = heatmap_color(ui, ucell.avg_voltage, cell_voltage, heatmap_delta);

        let cell_pos = pos + Vec2::new(0.0, i as f32 * cell_size.y);
        let mut rect = Rect::from_min_size(cell_pos, cell_size);
        ui.painter().rect_filled(rect, Rounding::none(), bg_color);

        let font_size = (cell_size.x + cell_size.y) / 8.0;

        ui.allocate_ui_at_rect(rect, |ui| {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new(cell_voltage.to_string())
                        .font(FontId::new(font_size, FontFamily::Monospace)),
                );
            });
        });

        rect.min.y += cell_size.y / 2.0;
        rect.max.x -= 10.0;
        ui.allocate_ui_at_rect(rect, |ui| {
            ui.with_layout(Layout::right_to_left(), |ui| {
                ui.label(
                    RichText::new((cell_index + 1).to_string())
                        .font(FontId::new(font_size / 2.0, FontFamily::Monospace)),
                )
            });
        });
    }

    for i in 0..9 {
        let cell_index = offset + i + 9;
        let cell_voltage = ucell
            .cell_voltage
            .get(cell_index)
            .copied()
            .unwrap_or(u16::MAX);
        let bg_color = heatmap_color(ui, ucell.avg_voltage, cell_voltage, heatmap_delta);

        let cell_pos = pos + Vec2::new(cell_size.x, i as f32 * cell_size.y);
        let mut rect = Rect::from_min_size(cell_pos, cell_size);
        ui.painter().rect_filled(rect, Rounding::none(), bg_color);

        let font_size = (cell_size.x + cell_size.y) / 8.0;

        ui.allocate_ui_at_rect(rect, |ui| {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new(cell_voltage.to_string())
                        .font(FontId::new(font_size, FontFamily::Monospace)),
                );
            });
        });

        rect.min.y += cell_size.y / 2.0;
        rect.max.x -= 10.0;
        ui.allocate_ui_at_rect(rect, |ui| {
            ui.with_layout(Layout::right_to_left(), |ui| {
                ui.label(
                    RichText::new((cell_index + 1).to_string())
                        .font(FontId::new(font_size / 2.0, FontFamily::Monospace)),
                )
            });
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
                    self.request = Some(fetch(&self.ip, self.safe));
                    self.last_poll = now;
                }
            }
        }
    }
}

fn heatmap_color(ui: &Ui, avg: u16, cell: u16, delta: f32) -> Color32 {
    if ui.style().visuals.dark_mode {
        const BG: u8 = 0x20;
        const RANGE: f32 = (255 - BG) as f32;
        let diff = ((cell as f32 - avg as f32) / (delta / 2.0)).clamp(-1.0, 1.0);
        if diff < 0.0 {
            let r = (-RANGE * diff) as u8 + BG;
            Color32::from_rgb(r, BG, BG)
        } else {
            let b = (RANGE * diff) as u8 + BG;
            Color32::from_rgb(BG, BG, b)
        }
    } else {
        const BG: u8 = 0xf0;
        const RANGE: f32 = BG as f32;
        let diff = ((cell as f32 - avg as f32) / (delta / 2.0)).clamp(-1.0, 1.0);
        if diff < 0.0 {
            let gb = BG - (-RANGE * diff) as u8;
            Color32::from_rgb(BG, gb, gb)
        } else {
            let rg = BG - (RANGE * diff) as u8;
            Color32::from_rgb(rg, rg, BG)
        }
    }
}

fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
