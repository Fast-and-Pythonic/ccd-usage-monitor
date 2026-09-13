//! Where the windows were left, in a file the machine owns.
//!
//! Three numbers-pairs that used to live in `config.toml`: the pill's position
//! and the settings window's position and size. They are written by dragging,
//! which is to say constantly, and that is the whole reason they are here —
//! a configuration file rewritten on every drag is one nobody can put a
//! comment in, because the comment does not survive the next nudge of the
//! window.
//!
//! So the split is by author rather than by subject: `config.toml` is what a
//! person tells the widget, `window.json` is what the widget remembers about
//! itself. Nobody is expected to read this one, which is why it is JSON and
//! why it sits next to `state.json` rather than beside the config.
//!
//! Written whole and swapped in, like the state file and the history log.
//! There is one writer, so there is no lock: the widget is the only thing that
//! ever has an opinion about where its own window is.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Geometry {
   /// The pill's top-left in logical pixels. `None` until the first drag.
   ///
   /// Trusted as saved: a negative coordinate is perfectly legitimate on a
   /// multi-monitor desktop, so nothing here second-guesses it. The recovery
   /// for a widget left off-screen is the tray's "Reset position", not a
   /// startup heuristic that would fight a legitimate layout.
   pub position: Option<[f32; 2]>,
   pub settings_pos: Option<[f32; 2]>,
   pub settings_size: Option<[f32; 2]>,
}

pub fn path() -> PathBuf {
   crate::state::state_dir().join("window.json")
}

pub fn load() -> Geometry {
   match fs::read_to_string(path()) {
      Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
      Err(_) => Geometry::default(),
   }
}

/// Errors are swallowed on purpose, as everywhere else that writes a file the
/// user never asked for: failing to remember a window position is not worth
/// interrupting anyone over, and the next drag will try again.
pub fn save(geom: &Geometry) {
   let dir = crate::state::state_dir();
   if fs::create_dir_all(&dir).is_err() {
      return;
   }
   let tmp = dir.join(format!("window.{}.tmp", std::process::id()));
   let Ok(text) = serde_json::to_string_pretty(geom) else {
      return;
   };
   let written = fs::File::create(&tmp).and_then(|mut f| {
      f.write_all(text.as_bytes())?;
      f.sync_all()
   });
   if written.is_ok() {
      let _ = fs::rename(&tmp, path());
   } else {
      let _ = fs::remove_file(&tmp);
   }
}

#[cfg(test)]
mod tests {
   use super::*;

   #[test]
   fn an_absent_file_is_a_widget_that_has_never_been_dragged() {
      // Not an error and not a default position: "nowhere in particular" is
      // a legitimate answer, and the window manager picks.
      let geom: Geometry = serde_json::from_str("{}").unwrap();
      assert_eq!(geom, Geometry::default());
      assert!(geom.position.is_none());
   }

   #[test]
   fn a_file_written_before_a_field_existed_still_loads() {
      // The same rule the config follows: a missing key is the default, so
      // adding a remembered rectangle later cannot strand anyone.
      let geom: Geometry = serde_json::from_str(r#"{"position":[60.0,60.0]}"#).unwrap();
      assert_eq!(geom.position, Some([60.0, 60.0]));
      assert!(geom.settings_pos.is_none());
   }

   #[test]
   fn a_negative_coordinate_survives_the_round_trip() {
      // A monitor left of the primary one gives negative coordinates, and
      // they are correct. Anything that "fixed" them would move the widget
      // off the screen the user actually put it on.
      let geom = Geometry {
         position: Some([-1720.5, 40.0]),
         settings_pos: Some([-1500.0, 200.0]),
         settings_size: Some([360.0, 470.0]),
      };
      let text = serde_json::to_string(&geom).unwrap();
      assert_eq!(serde_json::from_str::<Geometry>(&text).unwrap(), geom);
   }
}
