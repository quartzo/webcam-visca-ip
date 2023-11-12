use rusqlite::Connection;
use crate::auto_uvc;
use crate::uvierror::{UVIResult, UVIError};
use std::fs;
use dirs;
use std::sync::OnceLock;
use tokio::sync::{mpsc,oneshot};
use std::thread;

static PRESET_CHAN: OnceLock<mpsc::Sender<Box<dyn FnOnce(&rusqlite::Connection) -> () + Send>>> = OnceLock::new();

pub fn prepare_preset_db() -> UVIResult<()> {
  if let Some(_) = PRESET_CHAN.get() {
    return Ok(());
  }
  let (send_cmd, mut recv_cmd) = mpsc::channel(100);
  PRESET_CHAN.set(send_cmd).map_err(|_x| UVIError::AsyncChannelNoSender)?;

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
  thread::spawn(move || {
    while let Some(fnexe) = recv_cmd.blocking_recv() {
      fnexe(&conn);
    }
  });
  Ok(())
}

pub async fn clear(ncam: u8, npreset: u8) -> UVIResult<()> {
  let preset_send_cmd = PRESET_CHAN.get().ok_or(UVIError::AsyncChannelNoSender)?;
  let (respond_to, recv) = oneshot::channel();
  let fnrec = Box::new(move |conn: &rusqlite::Connection| {
    let r = conn.execute(
      "DELETE FROM Presets WHERE ncam=?1 AND preset=?2;",
      (&(ncam as i64), &(npreset as i64)),
    ).map(|_| ()).map_err(UVIError::RusqliteError);
    let _ = respond_to.send(r);
  });
  let _ = preset_send_cmd.send(fnrec).await;
  Ok(recv.await.expect("Actor task has been killed")?)
}

pub async fn record(ncam: u8, npreset: u8, p: auto_uvc::Preset) -> UVIResult<()> {
  let preset_send_cmd = PRESET_CHAN.get().ok_or(UVIError::AsyncChannelNoSender)?;
  let (respond_to, recv) = oneshot::channel();
  let fnrec = Box::new(move |conn: &rusqlite::Connection| {
    let r = conn.execute(
      "INSERT OR REPLACE INTO Presets (ncam,preset,
        pan, tilt, zoom,
        focusauto, focus, whitebalauto, temperature) 
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9);",
      (&(ncam as i64), &(npreset as i64), &p.pan, &p.tilt, &p.zoom,
        &p.focusauto,&p.focus,&p.whitebalauto,&p.temperature),
    ).map(|_| ()).map_err(UVIError::RusqliteError);
    let _ = respond_to.send(r);
  });
  let _ = preset_send_cmd.send(fnrec).await;
  Ok(recv.await.expect("Actor task has been killed")?)
}

pub async fn recover(ncam: u8, npreset: u8) -> UVIResult<Option<auto_uvc::Preset>> {
  let preset_send_cmd = PRESET_CHAN.get().ok_or(UVIError::AsyncChannelNoSender)?;
  let (respond_to, recv) = oneshot::channel();
  let fnrec = Box::new(move |conn: &rusqlite::Connection| {
    let r = match conn.query_row(
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
    };
    let _ = respond_to.send(r);
  });
  let _ = preset_send_cmd.send(fnrec).await;
  Ok(recv.await.expect("Actor task has been killed")?)
}
