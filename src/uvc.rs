use std::fmt;
use crate::uvierror::UVIError;

#[cfg(all(not(feature="uvcmock"), target_os = "linux"))]
use crate::uvc_linux as uvci;
#[cfg(all(not(feature="uvcmock"), target_os = "windows"))]
use crate::uvc_win as uvci;
#[cfg(feature="uvcmock")]
use crate::uvc_mock as uvci;

#[derive(Debug,Clone)]
pub enum ControlType {
    Integer,
    Boolean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CamControl {
    PanAbsolute,
    TiltAbsolute,
    ZoomAbsolute,
    FocusAbsolute,
    FocusAuto,
    WhiteBalanceTemperature,
    WhiteBalanceTemperatureAuto,
}

impl fmt::Display for CamControl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CamControl::PanAbsolute => write!(f, "pan_absolute"),
            CamControl::TiltAbsolute => write!(f, "tilt_absolute"),
            CamControl::ZoomAbsolute => write!(f, "zoom_absolute"),
            CamControl::FocusAbsolute => write!(f, "focus_absolute"),
            CamControl::FocusAuto => write!(f, "focus_automatic_continuous"),
            CamControl::WhiteBalanceTemperature => write!(f, "white_balance_temperature"),
            CamControl::WhiteBalanceTemperatureAuto => write!(f, "white_balance_automatic"),
        }
    }
}

#[derive(Debug,Clone)]
pub struct Description {
    pub typ: ControlType,
    pub minimum: i64,
    pub maximum: i64,
    pub step: u64,
    pub default: i64,
}

use tokio::sync::{mpsc,oneshot};

#[derive(Debug)]
pub enum UVCCmd {
    GetCtrlDescr(CamControl, oneshot::Sender<Result<Description, UVIError>>),
    SetCtrl(CamControl, i64, oneshot::Sender<Result<(), UVIError>>),
    GetCtrl(CamControl, oneshot::Sender<Result<i64, UVIError>>),
}

#[derive(Debug)]
pub struct Camera {
    channel: mpsc::Sender<UVCCmd>,
    ncam: u8,
    card: String,
    pub bus: String,
}

impl fmt::Display for Camera {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NCam {} Card {} Bus {}", self.ncam, self.card, self.bus)
    }
}

impl Camera {
    async fn send(&self, cmd: UVCCmd) -> Result<(), UVIError> {
        self.channel.send(cmd).await.map_err(|_x| UVIError::AsyncChannelClosed)
    }
    pub async fn get_ctrl_descr(&self, camctrl: CamControl) -> Result<Description, UVIError> {
        let (s, r) = oneshot::channel();
        self.send(UVCCmd::GetCtrlDescr(camctrl, s)).await?;
        r.await.map_err(|_x| UVIError::AsyncChannelNoSender)?
    }
    pub async fn set_ctrl(&self, camctrl: CamControl, vl: i64) -> Result<(), UVIError> {
        let (s, r) = oneshot::channel();
        self.send(UVCCmd::SetCtrl(camctrl, vl, s)).await?;
        r.await.map_err(|_x| UVIError::AsyncChannelNoSender)?
    } 
    pub async fn get_ctrl(&self, camctrl: CamControl) -> Result<i64, UVIError> {
        let (s, r) = oneshot::channel();
        self.send(UVCCmd::GetCtrl(camctrl, s)).await?;
        r.await.map_err(|_x| UVIError::AsyncChannelNoSender)?
    }
}

pub async fn find_camera(ncam: u8) -> Result<Camera, UVIError> {
    let (send_find, recv_find) = oneshot::channel();
    let (send_cmd, recv_cmd) = mpsc::channel(100);
    uvci::run_handler(ncam, send_find, recv_cmd);
    let (card, bus) = recv_find.await.map_err(|_x| UVIError::AsyncChannelNoSender)??;
    Ok(Camera {
        channel: send_cmd,
        ncam: ncam,
        card: card,
        bus: bus,
    })
}