//! Screens that are built in later milestones.

use eframe::egui;

use crate::app::{App, Nav};
use crate::widgets;

pub fn show(_app: &mut App, ui: &mut egui::Ui, nav: Nav) {
    ui.heading(nav.label());
    widgets::dim(ui, "This screen is under construction.");
}
