use std::io;
use rusqlite;
#[cfg(target_os = "windows")]
use nokhwa;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UVIError {
  #[error("Couldn't manipulate config directories")]
  BadDirs,
  #[error("This camera control is not available for device")]
  CamControlNotFound,
  #[cfg(all(not(feature="uvcmock"),target_os = "linux"))]
  #[error("This camera control uses unknown value type")]
  UnknownCameraControlValue,
  #[error("Couldn't access camera device")]
  CameraNotFound,
  #[error("Sending to a closed channel")]
  AsyncChannelClosed,
  #[error("Receiving from a closed channel")]
  AsyncChannelNoSender,
  #[error("rusqlite error")]
  RusqliteError(#[from] rusqlite::Error),
  #[error("std::io error")]
  IoError(#[from] io::Error),
  #[cfg(target_os = "windows")]
  #[error("Nokhwa error")]
  NokhwaError(#[from] nokhwa::NokhwaError),
}

pub type UVIResult<T> = std::result::Result<T, UVIError>;
