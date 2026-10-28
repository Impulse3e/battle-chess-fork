//! Боевые шахматы: шахматы, в которых у фигур есть атака и здоровье.
//! Доска и фигуры — пиксель-арт в духе Into the Breach, звуки — с lichess.org.

// В релизной сборке не открываем консольное окно рядом с игрой.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod orbit;
mod balance;
mod editor;
mod game;
mod net;

mod settings;
mod sound;
mod theme;

use eframe::egui;

fn main() -> eframe::Result {
    let settings = settings::Settings::load();
    let viewport = egui::ViewportBuilder::default()
        .with_title(concat!("Боевые шахматы ", env!("CARGO_PKG_VERSION")))
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([900.0, 560.0])
        .with_icon(assets::window_icon());
    let options = eframe::NativeOptions {
        viewport,
        // VSync включается только при запуске, поэтому смена режима — после перезапуска.
        vsync: settings.frame_rate.vsync(),
        ..Default::default()
    };
    eframe::run_native(
        "Боевые шахматы",
        options,
        Box::new(move |cc| Ok(Box::new(app::BattleChessApp::new(cc, settings)))),
    )
}
