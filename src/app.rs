use std::time::{Duration, SystemTime, UNIX_EPOCH};

use egui::style::Margin;
use egui::{
    menu, Align, CentralPanel, Color32, DragValue, FontFamily, FontId, Frame, Grid, Layout, Rect,
    RichText, Rounding, ScrollArea, SidePanel, TopBottomPanel, Ui, Vec2,
};

use serde::{Deserialize, Serialize};

use crate::api::{self, fetch, Data, Request, Tcell, Ucell};

const STACK_POS: [(f32, f32, Side); 8] = [
    (2.0, 1.0, Side::Right),
    (2.0, 0.0, Side::Right),
    (3.0, 0.0, Side::Right),
    (3.0, 1.0, Side::Right),
    (0.0, 1.0, Side::Left),
    (0.0, 0.0, Side::Left),
    (1.0, 0.0, Side::Left),
    (1.0, 1.0, Side::Left),
];

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct DashboardApp {
    pub safe: bool,
    pub ip: String,
    pub poll_rate: usize,
    pub voltage_heatmap_delta: f32,
    pub temp_heatmap_delta: f32,
    pub relative_heatmap: bool,
    #[serde(skip)]
    pub last_poll: u128,
    #[serde(skip)]
    request: Option<Request>,
    #[serde(skip)]
    pub data: Option<Data>,
    #[serde(skip)]
    pub error: Option<api::Error>,
}

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

impl Default for DashboardApp {
    fn default() -> Self {
        Self {
            safe: true,
            ip: "http://192.168.0.200".into(),
            poll_rate: 1000,
            voltage_heatmap_delta: 100.0,
            temp_heatmap_delta: 5.0,
            relative_heatmap: false,
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
        if ctx.input(|i| i.key_down(egui::Key::V) && i.key_pressed(egui::Key::W)) {
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

                ui.label("Volatge heatmap delta");
                ui.add(
                    DragValue::new(&mut self.voltage_heatmap_delta)
                        .clamp_range(5.0..=1000.0)
                        .speed(1.0),
                );

                ui.label("Temperature heatmap delta");
                ui.add(
                    DragValue::new(&mut self.temp_heatmap_delta)
                        .clamp_range(0.5..=25.0)
                        .speed(0.1),
                );

                ui.label("Relative heatmap");
                ui.checkbox(&mut self.relative_heatmap, "");

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
                            Grid::new("stats_container").show(ui, |ui| side_panel(ui, data));
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
                let temp_size = size * Vec2::new(1.0, 0.2);
                ui.allocate_ui_at_rect(Rect::from_min_size(pos, temp_size), |ui| {
                    draw_temps(ui, data, self);
                });

                let stacks_pos = pos + Vec2::new(pos.x, pos.y + temp_size.y);
                let stacks_size = Vec2::new(size.x, size.y - temp_size.y);
                ui.allocate_ui_at_rect(Rect::from_min_size(stacks_pos, stacks_size), |ui| {
                    draw_stacks(ui, data, self);
                });
            }
        });
    }
}

fn side_panel(ui: &mut Ui, data: &Data) {
    let ucell = &data.ucell;

    field(ui, "Current", data.main.current.to_string(), "mA");
    field(ui, "Voltage", format!("{:.3}", data.main.voltage), "V");
    ui.end_row();

    heading(ui, "Both accumulators");
    field(ui, "Min cell voltage", ucell.overall.min_voltage, "mV");
    field(ui, "Avg cell voltage", ucell.overall.avg_voltage, "mV");
    field(ui, "Max cell voltage", ucell.overall.max_voltage, "mV");
    field(ui, "Delta cell voltage", ucell.overall.delta_voltage, "mV");
    ui.end_row();

    heading(ui, "Right accumulator");
    field(ui, "Min cell voltage", ucell.right.min_voltage, "mV");
    field(ui, "Avg cell voltage", ucell.right.avg_voltage, "mV");
    field(ui, "Max cell voltage", ucell.right.max_voltage, "mV");
    field(ui, "Delta cell voltage", ucell.right.delta_voltage, "mV");
    ui.end_row();

    heading(ui, "Left accumulator");
    field(ui, "Min cell voltage", ucell.left.min_voltage, "mV");
    field(ui, "Avg cell voltage", ucell.left.avg_voltage, "mV");
    field(ui, "Max cell voltage", ucell.left.max_voltage, "mV");
    field(ui, "Delta cell voltage", ucell.left.delta_voltage, "mV");
    ui.end_row();

    field(ui, "Min temperature", data.main.temp_min, "°C");
    field(ui, "Avg temperature", data.main.temp_avg, "°C");
    field(ui, "Max temperature", data.main.temp_max, "°C");
    field(ui, "Master temperature", data.main.temp_master, "°C");
    ui.end_row();

    field(ui, "#Slaves", ucell.num_slaves, "");
    field(ui, "#Cells", ucell.num_cells, "");
    field(ui, "#Cells / #Slaves", ucell.num_cells_per_slave, "");
    field(ui, "#Temperature sensors", ucell.num_temp_sensors, "");
    field(ui, "#Safe resistors", ucell.num_safe_resistors, "");
}

fn heading(ui: &mut Ui, name: &str) {
    ui.heading(name);
    ui.end_row();
}

fn field(ui: &mut Ui, name: &str, value: impl ToString, unit: &str) {
    ui.label(name);
    ui.label(value.to_string());
    ui.label(unit);
    ui.end_row();
}

fn draw_temps(ui: &mut Ui, data: &Data, app: &DashboardApp) {
    let pos = ui.cursor().min;
    let size = ui.available_size();
    let stack_size = size / Vec2::new(4.0, 2.0);

    for (i, (x, y, side)) in STACK_POS.iter().enumerate() {
        let stack_pos = pos + Vec2::new(x * stack_size.x, y * stack_size.y);
        let stack_rect = Rect::from_min_size(stack_pos, stack_size);
        let offset = i * 2;
        ui.allocate_ui_at_rect(stack_rect, |ui| {
            draw_temp(ui, &data.tcell, offset, app, *side);
        });
    }
}

fn draw_temp(ui: &mut Ui, tcell: &Tcell, offset: usize, app: &DashboardApp, side: Side) {
    let pos = ui.cursor().min;
    let cell_size = ui.available_size() / Vec2::new(2.0, 1.0);
    let avg = if app.relative_heatmap {
        match side {
            Side::Left => tcell.left.avg_temp,
            Side::Right => tcell.right.avg_temp,
        }
    } else {
        tcell.overall.avg_temp
    };

    for i in 0..2 {
        let cell_index = offset + i;
        let cell_temp = tcell.temp.get(cell_index).copied().unwrap_or(f32::MAX);
        let bg_color = heatmap_color(ui, avg, cell_temp, app.temp_heatmap_delta);

        let cell_pos = pos + Vec2::new(i as f32 * cell_size.x, 0.0);
        let mut rect = Rect::from_min_size(cell_pos, cell_size);
        ui.painter().rect_filled(rect, Rounding::ZERO, bg_color);

        let font_size = (cell_size.x + cell_size.y) / 8.0;

        ui.allocate_ui_at_rect(rect, |ui| {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new(cell_temp.to_string())
                        .font(FontId::new(font_size, FontFamily::Monospace)),
                );
            });
        });

        rect.min.y += cell_size.y / 2.0;
        rect.max.x -= 10.0;
        ui.allocate_ui_at_rect(rect, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new((cell_index + 1).to_string())
                        .font(FontId::new(font_size / 2.0, FontFamily::Monospace)),
                )
            });
        });
    }
}

fn draw_stacks(ui: &mut Ui, data: &Data, app: &DashboardApp) {
    let pos = ui.cursor().min;
    let size = ui.available_size();
    let stack_size = size / Vec2::new(4.0, 2.0);

    for (i, (x, y, side)) in STACK_POS.iter().enumerate() {
        let stack_pos = pos + Vec2::new(x * stack_size.x, y * stack_size.y);
        let stack_rect = Rect::from_min_size(stack_pos, stack_size);
        let offset = i * 18;
        ui.allocate_ui_at_rect(stack_rect, |ui| {
            draw_stack(ui, &data.ucell, offset, app, *side)
        });
    }
}

fn draw_stack(ui: &mut Ui, ucell: &Ucell, offset: usize, app: &DashboardApp, side: Side) {
    let pos = ui.cursor().min;
    let cell_size = ui.available_size() / Vec2::new(2.0, 9.0);
    let avg = if app.relative_heatmap {
        match side {
            Side::Left => ucell.left.avg_voltage,
            Side::Right => ucell.right.avg_voltage,
        }
    } else {
        ucell.overall.avg_voltage
    };

    for i in 0..9 {
        let cell_index = offset + (8 - i);
        let cell_voltage = ucell
            .cell_voltage
            .get(cell_index)
            .copied()
            .unwrap_or(u16::MAX);
        let bg_color = heatmap_color(
            ui,
            avg as f32,
            cell_voltage as f32,
            app.voltage_heatmap_delta,
        );

        let cell_pos = pos + Vec2::new(0.0, i as f32 * cell_size.y);
        let mut rect = Rect::from_min_size(cell_pos, cell_size);
        ui.painter().rect_filled(rect, Rounding::ZERO, bg_color);

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
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
        let bg_color = heatmap_color(
            ui,
            avg as f32,
            cell_voltage as f32,
            app.voltage_heatmap_delta,
        );

        let cell_pos = pos + Vec2::new(cell_size.x, i as f32 * cell_size.y);
        let mut rect = Rect::from_min_size(cell_pos, cell_size);
        ui.painter().rect_filled(rect, Rounding::ZERO, bg_color);

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
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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

fn heatmap_color(ui: &Ui, avg: f32, cell: f32, delta: f32) -> Color32 {
    if ui.style().visuals.dark_mode {
        const BG: u8 = 0x20;
        const RANGE: f32 = (255 - BG) as f32;
        let diff = ((cell - avg) / (delta / 2.0)).clamp(-1.0, 1.0);
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
        let diff = ((cell - avg) / (delta / 2.0)).clamp(-1.0, 1.0);
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
