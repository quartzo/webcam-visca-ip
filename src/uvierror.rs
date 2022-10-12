use std::io;
use rusqlite;
use std::fmt;
#[cfg(target_os = "windows")]
use nokhwa;

#[derive(Debug)]
pub enum UVIError {
  BadDirs,
  CamControlNotFound,
  #[cfg(target_os = "linux")]
  UnknownCameraControlValue,
  CameraNotFound,
  AsyncChannelClosed,
  AsyncChannelNoSender,
  RusqliteError(rusqlite::Error),
  IoError(io::Error),
  #[cfg(target_os = "windows")]
  NokhwaError(nokhwa::NokhwaError),
}

impl fmt::Display for UVIError {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match *self {
      UVIError::BadDirs => write!(f, "Couldn't manipulate config directories"),
      UVIError::CamControlNotFound => write!(f, "This camera control is not available for device"),
      #[cfg(target_os = "linux")]
      UVIError::UnknownCameraControlValue => write!(f, "This camera control uses unknown value type"),
      UVIError::CameraNotFound => write!(f, "Couldn't access camera device"),
      UVIError::AsyncChannelClosed => write!(f, "Sending to a closed channel"),
      UVIError::AsyncChannelNoSender => write!(f, "Receiving from a closed channel"),
      // This is a wrapper, so defer to the underlying types' implementation of `fmt`.
      UVIError::RusqliteError(ref e) => e.fmt(f),
      UVIError::IoError(ref e) => e.fmt(f),
      #[cfg(target_os = "windows")]
      UVIError::NokhwaError(ref e) => e.fmt(f),
    }
  }
}

impl From<rusqlite::Error> for UVIError {
  fn from(err: rusqlite::Error) -> Self {
      UVIError::RusqliteError(err)
  }
}
impl From<io::Error> for UVIError {
  fn from(err: io::Error) -> Self {
    UVIError::IoError(err)
  }
}
#[cfg(target_os = "windows")]
impl From<nokhwa::NokhwaError> for UVIError {
  fn from(err: nokhwa::NokhwaError) -> Self {
    UVIError::NokhwaError(err)
  }
}
