use rusqlite::{Connection, Result};
use crate::auto_uvc;
use crate::uvierror::UVIError;
use std::fs;
use dirs;
use std::sync::OnceLock;
use tokio::sync::{mpsc,oneshot};
use std::thread;

/*
pub struct Preset {
  pub pan: i64, pub tilt: i64, pub zoom: i64,
  pub focusauto: bool, pub focus: i64, 
  pub whitebalauto: bool, pub temperature: i64
}
*/

pub struct PresetDBActor {
  receiver: mpsc::Receiver<PresetDBActorMessage>,
  conn: Connection
}
enum PresetDBActorMessage {
  Clear{
    ncam: u8, npreset: u8,
    respond_to: oneshot::Sender<Result<(), UVIError>>,
  },
  Record{
    ncam: u8, npreset: u8, preset: auto_uvc::Preset,
    respond_to: oneshot::Sender<Result<(), UVIError>>,
  },
  Recover{
    ncam: u8, npreset: u8,
    respond_to: oneshot::Sender<Result<Option<auto_uvc::Preset>, UVIError>>,
  },
}

impl PresetDBActor {
  fn handle_message(&mut self, msg: PresetDBActorMessage) {
    match msg {
      PresetDBActorMessage::Clear{ncam, npreset, respond_to} => {
        let r = self.conn.execute(
          "DELETE FROM Presets WHERE ncam=?1 AND preset=?2;",
          (&(ncam as i64), &(npreset as i64)),
        ).map(|_| ()).map_err(UVIError::RusqliteError);
        let _ = respond_to.send(r);
      },
      PresetDBActorMessage::Record{ncam, npreset, preset, respond_to} => {
        let p = preset;
        let r = self.conn.execute(
          "INSERT OR REPLACE INTO Presets (ncam,preset,
            pan, tilt, zoom,
            focusauto, focus, whitebalauto, temperature) 
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9);",
          (&(ncam as i64), &(npreset as i64), &p.pan, &p.tilt, &p.zoom,
            &p.focusauto,&p.focus,&p.whitebalauto,&p.temperature),
        ).map(|_| ()).map_err(UVIError::RusqliteError);
        let _ = respond_to.send(r);
      },
      PresetDBActorMessage::Recover{ncam, npreset, respond_to} => {
        let r = match self.conn.query_row(
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
      },
    }
  }
  fn run(mut self) {
    thread::spawn(move || {
      while let Some(msg) = self.receiver.blocking_recv() {
        self.handle_message(msg);
      }
    });
  }
  fn new() -> Result<mpsc::Sender<PresetDBActorMessage>, UVIError> {
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
    let (send_cmd, recv_cmd) = mpsc::channel(100);
    let slf = PresetDBActor{receiver:recv_cmd, conn};
    slf.run();
    Ok(send_cmd)
  }
}

static PRESET_CHAN: OnceLock<mpsc::Sender<PresetDBActorMessage>> = OnceLock::new();

pub fn prepare_preset_db() -> Result<(), UVIError> {
  if let Some(_) = PRESET_CHAN.get() {
    return Ok(());
  }
  let preset_send_cmd = PresetDBActor::new()?;
  PRESET_CHAN.set(preset_send_cmd).map_err(|_x| UVIError::AsyncChannelNoSender)?;
  Ok(())
}

pub async fn clear(ncam: u8, npreset: u8) -> Result<(), UVIError> {
  let preset_send_cmd = PRESET_CHAN.get().ok_or(UVIError::AsyncChannelNoSender)?;
  let (send, recv) = oneshot::channel();
  let msg = PresetDBActorMessage::Clear {
    ncam: ncam, npreset: npreset,
    respond_to: send,
  };
  let _ = preset_send_cmd.send(msg).await;
  Ok(recv.await.expect("Actor task has been killed")?)
}

pub async fn record(ncam: u8, npreset: u8, p: auto_uvc::Preset) -> Result<(), UVIError> {
  let preset_send_cmd = PRESET_CHAN.get().ok_or(UVIError::AsyncChannelNoSender)?;
  let (send, recv) = oneshot::channel();
  let msg = PresetDBActorMessage::Record {
    ncam: ncam, npreset: npreset, preset: p,
    respond_to: send,
  };
  let _ = preset_send_cmd.send(msg).await;
  Ok(recv.await.expect("Actor task has been killed")?)
}

pub async fn recover(ncam: u8, npreset: u8) -> Result<Option<auto_uvc::Preset>, UVIError> {
  let preset_send_cmd = PRESET_CHAN.get().ok_or(UVIError::AsyncChannelNoSender)?;
  let (send, recv) = oneshot::channel();
  let msg = PresetDBActorMessage::Recover {
    ncam: ncam, npreset: npreset,
    respond_to: send,
  };
  let _ = preset_send_cmd.send(msg).await;
  Ok(recv.await.expect("Actor task has been killed")?)
}
