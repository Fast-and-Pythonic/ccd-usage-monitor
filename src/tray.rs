//! The system tray icon and its menu.
//!
//! Nothing here touches egui — `tray-icon` talks to the OS directly — so this
//! module survives a change of GUI unchanged. The tray is the way back from
//! click-through mode, where the pill can no longer be right-clicked, which is
//! why the app refuses to offer that mode without it (A05).
//!
//! It is written as a *source of commands* rather than as a handler: the menu
//! knows which item was clicked, and nothing about what the click means. That
//! keeps the widget's state in one place and lets a different front end consume
//! the same events.

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// What the user asked for, in the widget's terms rather than the menu's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    ToggleVisible,
    OpenSettings,
    ToggleClickThrough,
    /// The only way back when a saved position lands on a monitor that no
    /// longer exists.
    ResetPosition,
    Quit,
}

/// The label of the show/hide item, which says what the click will *do*.
fn visible_label(visible: bool, ru: bool) -> &'static str {
    match (visible, ru) {
        (true, true) => "Скрыть",
        (false, true) => "Показать",
        (true, false) => "Hide",
        (false, false) => "Show",
    }
}

/// The click-through label, which says what the state *is*. A toggle that
/// reads as a state needs the state spelled out; the show/hide item above is
/// unambiguous without it.
fn pass_label(click_through: bool, ru: bool) -> &'static str {
    match (click_through, ru) {
        (true, true) => "Клики насквозь: вкл",
        (false, true) => "Клики насквозь: выкл",
        (true, false) => "Click-through: on",
        (false, false) => "Click-through: off",
    }
}

pub struct Tray {
    _tray: TrayIcon,
    visible_item: MenuItem,
    settings_item: MenuItem,
    pass_item: MenuItem,
    reset_item: MenuItem,
    quit_item: MenuItem,
    /// Which language the fixed labels were last written in. The two toggles
    /// are rewritten every tick anyway; the rest only when the language moves.
    labels_ru: Option<bool>,
}

/// A 32×32 donut in the accent colour, drawn rather than shipped as a file.
fn tray_image() -> Icon {
    const N: u32 = 32;
    let mut rgba = vec![0u8; (N * N * 4) as usize];
    let c = (N as f32 - 1.0) / 2.0;
    for y in 0..N {
        for x in 0..N {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            // Anti-aliased ring between radius 6 and 15.
            let outer = (15.0 - d).clamp(0.0, 1.0);
            let inner = (d - 6.0).clamp(0.0, 1.0);
            let a = (outer * inner * 255.0) as u8;
            let i = ((y * N + x) * 4) as usize;
            rgba[i] = 0x6F;
            rgba[i + 1] = 0xCF;
            rgba[i + 2] = 0x97;
            rgba[i + 3] = a;
        }
    }
    Icon::from_rgba(rgba, N, N).expect("32x32 rgba is a valid icon")
}

impl Tray {
    pub fn build(ru: bool) -> Option<Tray> {
        let menu = Menu::new();
        let visible_item = MenuItem::new(visible_label(true, ru), true, None);
        // Also the only way in while clicks fall through the pill.
        let settings_item = MenuItem::new(if ru { "Настройки" } else { "Settings" }, true, None);
        let pass_item = MenuItem::new(pass_label(false, ru), true, None);
        let reset_item = MenuItem::new(
            if ru { "Вернуть на экран" } else { "Reset position" },
            true,
            None,
        );
        let quit_item = MenuItem::new(if ru { "Выход" } else { "Quit" }, true, None);
        menu.append(&visible_item).ok()?;
        menu.append(&settings_item).ok()?;
        menu.append(&pass_item).ok()?;
        menu.append(&reset_item).ok()?;
        menu.append(&quit_item).ok()?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(tray_image())
            .with_tooltip("ccd-usage-monitor")
            .build()
            .ok()?;

        Some(Tray {
            _tray: tray,
            visible_item,
            settings_item,
            pass_item,
            reset_item,
            quit_item,
            labels_ru: None,
        })
    }

    /// Drain both global channels and say what was asked for.
    ///
    /// Called from wherever the app pumps state rather than from wherever it
    /// paints, so the menu keeps working while the window is hidden (G04).
    pub fn poll(&self) -> Vec<Command> {
        let mut out = Vec::new();

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::DoubleClick { .. } = event {
                out.push(Command::ToggleVisible);
            }
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = &event.id;
            if id == self.visible_item.id() {
                out.push(Command::ToggleVisible);
            } else if id == self.settings_item.id() {
                out.push(Command::OpenSettings);
            } else if id == self.pass_item.id() {
                out.push(Command::ToggleClickThrough);
            } else if id == self.reset_item.id() {
                out.push(Command::ResetPosition);
            } else if id == self.quit_item.id() {
                out.push(Command::Quit);
            }
        }

        out
    }

    /// Bring the menu's text in line with the state it describes.
    pub fn sync_labels(&mut self, visible: bool, click_through: bool, ru: bool) {
        self.visible_item.set_text(visible_label(visible, ru));
        self.pass_item.set_text(pass_label(click_through, ru));
        if self.labels_ru != Some(ru) {
            self.quit_item.set_text(if ru { "Выход" } else { "Quit" });
            self.settings_item
                .set_text(if ru { "Настройки" } else { "Settings" });
            self.reset_item
                .set_text(if ru { "Вернуть на экран" } else { "Reset position" });
            self.labels_ru = Some(ru);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_show_hide_item_names_the_action_not_the_state() {
        // Visible now means the click hides it, in either language.
        assert_eq!(visible_label(true, false), "Hide");
        assert_eq!(visible_label(true, true), "Скрыть");
        assert_eq!(visible_label(false, false), "Show");
        assert_eq!(visible_label(false, true), "Показать");
    }

    #[test]
    fn the_click_through_item_names_the_state_not_the_action() {
        // The opposite convention, deliberately: a toggle whose label read as
        // an action would not say which way it currently is.
        assert!(pass_label(true, false).ends_with("on"));
        assert!(pass_label(false, false).ends_with("off"));
        assert!(pass_label(true, true).ends_with("вкл"));
        assert!(pass_label(false, true).ends_with("выкл"));
    }
}
