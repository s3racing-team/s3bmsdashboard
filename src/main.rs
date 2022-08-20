#![windows_subsystem = "windows"]
use app::DashboardApp;

use eframe::NativeOptions;

mod api;
mod app;

const APP_NAME: &str = "s3bmsdashboard";

fn main() {
    let options = NativeOptions {
        drag_and_drop_support: true,
        follow_system_theme: true,
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|c| Box::new(DashboardApp::new(c))),
    );
}
