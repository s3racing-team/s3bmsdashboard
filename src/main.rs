#![windows_subsystem = "windows"]
use app::DashboardApp;

use eframe::NativeOptions;

mod api;
mod app;

const APP_NAME: &str = "s3bmsdashboard";

fn main() {
    let options = NativeOptions {
        follow_system_theme: true,
        ..Default::default()
    };
    let res = eframe::run_native(
        APP_NAME,
        options,
        Box::new(|c| Box::new(DashboardApp::new(c))),
    );
    if let Err(err) = res {
        println!("{err}");
    }
}
