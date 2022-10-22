use std::net;
use tokio::sync::oneshot;

#[derive(Debug)]
pub enum MainEvent {
  NewViscaCam(u8, u32, String),
  NewViscaConnection(u8, net::SocketAddr),
  LostViscaConnection(u8, net::SocketAddr),
  LostViscaCam(u8)
}

#[derive(Debug)]
pub struct PanTilt {
  pub pan: i64, // seconds of an angle (angle/3600) -170 to 170 degrees
  pub tilt: i64 // seconds of an angle (angle/3600) -30 to 30 degrees
}

#[derive(Debug)]
pub enum CamCmd {
  SetPresetNcam(u8),
  ResetPreset(u8),
  RecordPreset(u8),
  RecoverPreset(u8),
  Home(),
  MoveContinuous(PanTilt),
  MoveRelative(PanTilt),
  MoveAbsolute(PanTilt),
  ZoomContinuous(f64), // -1 to 1
  ZoomDirect(f64), // 0 to 1.0
  AutoFocus(bool),
  AutoFocusToggle(),
  FocusContinuous(f64), // -1 to 1
  FocusDirect(f64), // 1.0 (Near) - 0.0 (Far)
  FocusOnePushTrigger(),
  WhiteBalanceTrigger(),
  WhiteBalanceMode(u8),
  QueryPanTilt(oneshot::Sender<PanTilt>),
  QueryFocusMode(oneshot::Sender<bool>),
  QueryWhiteBalanceMode(oneshot::Sender<u8>),
  //Close()
}
