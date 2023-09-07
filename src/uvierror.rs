use std::io;
use rusqlite;
use std::net::AddrParseError;
#[cfg(target_os = "windows")]
use nokhwa;
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

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
  #[error("AddrParse error")]
  AddrParseError(#[from] AddrParseError),
  #[error("serde_json error")]
  SerdeJsonError(#[from] serde_json::Error),
  #[error("MPSC Send Error")]
  MPSCSendError,
  #[cfg(target_os = "windows")]
  #[error("Nokhwa error")]
  NokhwaError(#[from] nokhwa::NokhwaError),
}

impl<T> From<SendError<T>> for UVIError {
  fn from(_err: SendError<T>) -> Self {
      // Get details from the error you want,
      // or even implement for both T variants.
      Self::MPSCSendError
  }
}

pub type UVIResult<T> = std::result::Result<T, UVIError>;
