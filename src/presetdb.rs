use rusqlite::{Connection, Result};
use crate::auto_uvc;
use crate::uvierror::UVIError;
use std::fs;
use dirs;

#[derive(Debug)]
pub struct PresetDB {
  conn: Connection
}

/*
pub struct Preset {
  pub pan: i64, pub tilt: i64, pub zoom: i64,
  pub focusauto: bool, pub focus: i64, 
  pub whitebalauto: bool, pub temperature: i64
}
*/

pub async fn prepare_preset_db() -> Result<(), UVIError> {
  let conn = connect_preset_db()?;
  conn.conn.execute(
    r#"
    CREATE TABLE IF NOT EXISTS Presets (
      ncam INT, 
      preset INT, 
      pan INT, tilt INT, zoom INT,
      focusauto BOOL, focus INT,
      whitebalauto BOOL, temperature INT,
      PRIMARY KEY (ncam, preset)
    );"#,
    (),
  )?;
  Ok(())
}

pub fn connect_preset_db() -> Result<PresetDB, UVIError> {
  let mut path = dirs::config_dir().ok_or(UVIError::BadDirs)?;
  path.push("webcam-visca-ip");
  fs::create_dir_all(path.to_str().ok_or(UVIError::BadDirs)?)?;
  path.push("presets.db");
  let conn = Connection::open(path.to_str().ok_or(UVIError::BadDirs)?)?;
  Ok(PresetDB {conn:conn})
}

impl PresetDB {
  pub fn clear(&self, ncam: u8, npreset: u8) -> Result<(), UVIError> {
    self.conn.execute(
      "DELETE FROM Presets WHERE ncam=?1 AND preset=?2;",
      (&(ncam as i64), &(npreset as i64)),
    )?;
    Ok(())
  }
  pub fn record(&self, ncam: u8, npreset: u8, p: auto_uvc::Preset) -> Result<(), UVIError> {
    self.conn.execute(
      "INSERT OR REPLACE INTO Presets (ncam,preset,
        pan, tilt, zoom,
        focusauto, focus, whitebalauto, temperature) 
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9);",
      (&(ncam as i64), &(npreset as i64), &p.pan, &p.tilt, &p.zoom,
        &p.focusauto,&p.focus,&p.whitebalauto,&p.temperature),
    )?;
    Ok(())
  }
  pub fn recover(&self, ncam: u8, npreset: u8) -> Result<Option<auto_uvc::Preset>, UVIError> {
    match self.conn.query_row(
      "SELECT pan, tilt, zoom,
      focusauto, focus, whitebalauto, temperature 
      FROM Presets WHERE ncam=?1 and preset=?2;",
      (&(ncam as i64), &(npreset as i64)),
      |row| {
        Ok(auto_uvc::Preset {
          pan: row.get(0)?,
          tilt: row.get(1)?,
          zoom: row.get(2)?,
          focusauto: row.get(3)?,
          focus: row.get(4)?,
          whitebalauto: row.get(5)?,
          temperature: row.get(6)?
        })
      }
    ) {
      Ok(preset) => Ok(Some(preset)),
      Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
      Err(e) => Err(UVIError::RusqliteError(e))
    }
  }
}

