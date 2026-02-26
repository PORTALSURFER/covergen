//! Main-menu and export-popup state models.

mod export_popup;
mod main_popup;

pub(crate) use export_popup::{ExportMenuItem, ExportMenuState};
pub(crate) use main_popup::{MainMenuItem, MainMenuState, MAIN_MENU_WIDTH};

#[cfg(test)]
mod tests;
