use rusqlite::{Connection, Result};
use crate::auto_uvc;
use crate::uvierror::UVIError;
use std::fs;
use dirs;
use std::cell::OnceCell;
use std::rc::Rc;

thread_local! {
  static THCONN: OnceCell<Rc<Connection>> = OnceCell::new();
}

fn prepare_conn() -> Result<Rc<Connection>, UVIError> {
  THCONN.with(|thconn: &OnceCell<Rc<Connection>>| {
    if let Some(rc_conn) = thconn.get() {
      return Ok(rc_conn.clone());
    }
    let mut path = dirs::config_dir().ok_or(UVIError::BadDirs)?;
    path.push("webcam-visca-ip");
    fs::create_dir_all(path.to_str().ok_or(UVIError::BadDirs)?)?;
    path.push("presets.db");
    let conn = Connection::open(path.to_str().ok_or(UVIError::BadDirs)?)?;

    conn.execute(
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
    let rc_conn = Rc::new(conn);
    thconn.set(rc_conn.clone()).unwrap();
    Ok(rc_conn)
  })
}

#[derive(Debug)]
pub struct PresetDB {
  ncam: u8
}

/*
pub struct Preset {
  pub pan: i64, pub tilt: i64, pub zoom: i64,
  pub focusauto: bool, pub focus: i64, 
  pub whitebalauto: bool, pub temperature: i64
}
*/

pub async fn connect_preset_db(ncam: u8) -> Result<PresetDB, UVIError> {
  prepare_conn()?;
  Ok(PresetDB {ncam})
}

pub async fn  prepare_preset_db() -> Result<(), UVIError> {
  prepare_conn()?;
  Ok(())
}

impl PresetDB {
  pub async fn clear(&self, npreset: u8) -> Result<(), UVIError> {
    let dbconn = prepare_conn()?;
    dbconn.execute(
      "DELETE FROM Presets WHERE ncam=?1 AND preset=?2;",
      (&(self.ncam as i64), &(npreset as i64)),
    )?;
    Ok(())
  }
  pub async fn record(&self, npreset: u8, p: auto_uvc::Preset) -> Result<(), UVIError> {
    let dbconn = prepare_conn()?;
    dbconn.execute(
      "INSERT OR REPLACE INTO Presets (ncam,preset,
        pan, tilt, zoom,
        focusauto, focus, whitebalauto, temperature) 
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9);",
      (&(self.ncam as i64), &(npreset as i64), &p.pan, &p.tilt, &p.zoom,
        &p.focusauto,&p.focus,&p.whitebalauto,&p.temperature),
    )?;
    Ok(())
  }
  pub async fn recover(&self, npreset: u8) -> Result<Option<auto_uvc::Preset>, UVIError> {
    let dbconn = prepare_conn()?;
    match dbconn.query_row(
      "SELECT pan, tilt, zoom,
      focusauto, focus, whitebalauto, temperature 
      FROM Presets WHERE ncam=?1 and preset=?2;",
      (&(self.ncam as i64), &(npreset as i64)),
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
