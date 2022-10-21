use std::fmt;
use crate::uvierror::UVIError;

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
    #[cfg(target_os = "linux")]
    pub id: u32,
    pub typ: ControlType,
    #[cfg(target_os = "linux")]
    pub name: String,
    #[cfg(target_os = "windows")]
    pub kcontrol: nokhwa::KnownCameraControls,
    pub minimum: i64,
    pub maximum: i64,
    #[cfg(target_os = "windows")]
    sec2degree: bool,
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

#[cfg(target_os = "linux")]
mod uvci {
    use v4l::Device;
    use std::collections::HashMap;
    use tokio::sync::{mpsc,oneshot};
    use tokio::task;
    use super::*;
    
    pub struct CamInterno {
        dev: Device,
        ctrls: HashMap<CamControl, Description>
    }

    use crate::uvierror::UVIError;
    use v4l::control;
    use v4l::capability;

    impl CamControl {
        fn lowercase_cam_control_name(name: &str) -> String {
            let mut name_ = String::new();
            let mut part = String::new();
            for c in format!("{}_", name).chars() {
                if c.is_ascii_alphanumeric() {
                    part.push(c.to_ascii_lowercase());
                } else {
                    if part.len() > 0 {
                        if name_.len() > 0 { name_.push('_'); }
                        name_.push_str(&part);
                        part = String::new();
                    }
                }
            }
            return name_;
        }
        
        const VALUES: &'static [Self] = &[CamControl::PanAbsolute, CamControl::TiltAbsolute, 
            CamControl::ZoomAbsolute, CamControl::FocusAbsolute, CamControl::FocusAuto,
            CamControl::WhiteBalanceTemperature, CamControl::WhiteBalanceTemperatureAuto];
        fn find(name: &str) -> Option<CamControl> {
            let lwrcase = CamControl::lowercase_cam_control_name(name);
            for cam_control in CamControl::VALUES.iter() {
                if format!("{}", cam_control) == lwrcase {
                    return Some(*cam_control);
                }
            }
            None
        }
    }

    pub async fn find_camera(ncam: u8) -> Result<(CamInterno,String,String), UVIError> {
        let path = format!("/dev/video{}",ncam);
        let dev = Device::with_path(path)?;
        let caps = dev.query_caps()?;
        if !caps.capabilities.contains(capability::Flags::VIDEO_CAPTURE) {
            return Err(UVIError::CameraNotFound);
        };
        let controls = dev.query_controls()?;
        let mut cam = CamInterno {
            dev: dev,
            ctrls: HashMap::new()
        };
        for control in controls {
            match CamControl::find(&control.name) {
                None => (),
                Some(controle) => {
                    let typ = match control.typ {
                        control::Type::Integer => ControlType::Integer,
                        control::Type::Boolean => ControlType::Boolean,
                        _ => break
                    };
                    let descr = Description {
                        id: control.id,
                        typ: typ,
                        name: control.name,
                        minimum: control.minimum,
                        maximum: control.maximum,
                        step: control.step,
                        default: control.default,
                    };
                    cam.ctrls.insert(controle, descr);
                }
            }
        }
        Ok((cam, caps.card, caps.bus))
    }

    impl CamInterno {
        fn get_ctrl_descr(&self, camctrl: CamControl) -> Result<&Description, UVIError> {
            self.ctrls.get(&camctrl).ok_or(UVIError::CamControlNotFound)
        }
        fn set_ctrl(&self, camctrl: CamControl, vl: i64) -> Result<(), UVIError> {
            let ctrl = self.get_ctrl_descr(camctrl)?;
            self.dev.set_control(control::Control {
                id: ctrl.id,
                value: control::Value::Integer(vl)
            })?;
            Ok(())
        }
        fn get_ctrl(&self, camctrl: CamControl) -> Result<i64, UVIError> {
            let ctrl = self.get_ctrl_descr(camctrl)?;
            let val = self.dev.control(ctrl.id)?.value;
            match val {
                control::Value::Integer(n) => Ok((n as i32) as i64), // solve bug in the driver v4l
                control::Value::Boolean(n) if n == false => Ok(0),
                control::Value::Boolean(_) => Ok(1),
                _ => Err(UVIError::UnknownCameraControlValue)
            }
        }
        pub async fn run_command(&mut self, ev: UVCCmd) {
            match ev {
                UVCCmd::GetCtrlDescr(ctrlname, s) => {
                    s.send(self.get_ctrl_descr(ctrlname).map(|d| d.clone())).ok();
                },
                UVCCmd::SetCtrl(ctrlname, vl, s) => {
                    s.send(self.set_ctrl(ctrlname, vl)).ok();
                },
                UVCCmd::GetCtrl(ctrlname, s) => {
                    s.send(self.get_ctrl(ctrlname)).ok();
                } 
            }
        }
    }
    pub fn run_handler(ncam: u8, send_find: oneshot::Sender<Result<(String,String),UVIError>>,
            mut recv_cmd: mpsc::Receiver<UVCCmd>) {
        task::spawn(async move {
            match find_camera(ncam).await {
                Err(e) => {
                    send_find.send(Err(e)).ok();
                },
                Ok((mut cam, card, bus)) => {
                    send_find.send(Ok((card, bus))).ok();
                    while let Some(ev) = recv_cmd.recv().await {
                        cam.run_command(ev).await;
                    }
                }
            }
        });
    }
}

#[cfg(target_os = "windows")]
mod uvci {
    use nokhwa;
    use std::collections::HashMap;
    use crate::uvierror::UVIError;
    use nokhwa::KnownCameraControls::*;
    use tokio::sync::{mpsc,oneshot};
    use std::thread;
    use super::*;

    pub struct CamInterno {
        dev: nokhwa::Camera,
        ctrls: HashMap<CamControl, Description>
    }

    pub fn find_camera(ncam: u8) -> Result<(CamInterno, String, String), UVIError> {
        let camera = nokhwa::Camera::new(ncam.into(), None)?;
        let info = camera.info();
        let card = info.human_name();
        let bus = format!("#{}", ncam+1);
        let controls = camera.camera_controls_known_camera_controls()?;
        let mut ctrls = HashMap::new();
        for (kcontrol, control) in &controls {
            match kcontrol {
                Pan => {
                    ctrls.insert(CamControl::PanAbsolute, Description {
                        typ: ControlType::Integer,
                        kcontrol: *kcontrol,
                        minimum: (control.minimum_value()*3600).into(),
                        maximum: (control.maximum_value()*3600).into(),
                        sec2degree: true,
                        step: control.step() as u64,
                        default: control.default().into(),
                    });
                },
                Tilt => {
                    ctrls.insert(CamControl::TiltAbsolute, Description {
                        typ: ControlType::Integer,
                        kcontrol: *kcontrol,
                        minimum: (control.minimum_value()*3600).into(),
                        maximum: (control.maximum_value()*3600).into(),
                        sec2degree: true,
                        step: control.step() as u64,
                        default: control.default().into(),
                    });
                },
                Zoom => {
                    ctrls.insert(CamControl::ZoomAbsolute, Description {
                        typ: ControlType::Integer,
                        kcontrol: *kcontrol,
                        minimum: control.minimum_value().into(),
                        maximum: control.maximum_value().into(),
                        sec2degree: false,
                        step: control.step() as u64,
                        default: control.default().into(),
                    });
                },
                Focus => {
                    ctrls.insert(CamControl::FocusAbsolute, Description {
                        typ: ControlType::Integer,
                        kcontrol: *kcontrol,
                        minimum: control.minimum_value().into(),
                        maximum: control.maximum_value().into(),
                        sec2degree: false,
                        step: control.step() as u64,
                        default: control.default().into(),
                    });
                    ctrls.insert(CamControl::FocusAuto, Description {
                        typ: ControlType::Boolean,
                        kcontrol: *kcontrol,
                        minimum: 0,
                        maximum: 1,
                        sec2degree: false,
                        step: 1,
                        default: 1,
                    });
                },        
                WhiteBalance => {
                    ctrls.insert(CamControl::WhiteBalanceTemperature, Description {
                        typ: ControlType::Integer,
                        kcontrol: *kcontrol,
                        minimum: control.minimum_value().into(),
                        maximum: control.maximum_value().into(),
                        sec2degree: false,
                        step: control.step() as u64,
                        default: control.default().into(),
                    });
                    ctrls.insert(CamControl::WhiteBalanceTemperatureAuto, Description {
                        typ: ControlType::Boolean,
                        kcontrol: *kcontrol,
                        minimum: 0,
                        maximum: 1,
                        sec2degree: false,
                        step: 1,
                        default: 1,
                    });
                },
                //Brightness,Contrast,Hue,Saturation,Sharpness,Gamma,ColorEnable,
                //BacklightComp,Gain,Roll,Exposure,Iris,
                _ => ()
            }
        }
        if !ctrls.contains_key(&CamControl::PanAbsolute) || !ctrls.contains_key(&CamControl::TiltAbsolute) {
            return Err(UVIError::CameraNotFound);
        }
        let cam = CamInterno {
            dev: camera,
            ctrls: ctrls
        };
        Ok((cam, card, bus))
    }

    impl CamInterno {
        pub fn get_ctrl_descr(&self, camctrl: CamControl) -> Result<&Description, UVIError> {
            self.ctrls.get(&camctrl).ok_or(UVIError::CamControlNotFound)
        }
        pub fn set_ctrl(&mut self, camctrl: CamControl, mut vl: i64) -> Result<(), UVIError> {
            let ctrldescr = self.get_ctrl_descr(camctrl)?;
            let mut ctrl = self.dev.camera_control(ctrldescr.kcontrol)?;
            match ctrldescr.typ {
                ControlType::Integer => {
                    if ctrldescr.sec2degree { vl /= 3600; }
                    ctrl.set_value(vl as i32)?;
                },
                ControlType::Boolean => {
                    ctrl.set_active(if vl != 0 {true} else {false});
                }
            }
            self.dev.set_camera_control(ctrl)?;
            Ok(())
        } 
        pub fn get_ctrl(&self, camctrl: CamControl) -> Result<i64, UVIError> {
            let ctrldescr = self.get_ctrl_descr(camctrl)?;
            let res = self.dev.camera_control(ctrldescr.kcontrol)?;
            match ctrldescr.typ {
                ControlType::Integer => {
                    let mut vl = res.value().into();
                    if ctrldescr.sec2degree { vl *= 3600; }
                    Ok(vl)
                },
                ControlType::Boolean => {
                    match res.flag() {
                        nokhwa::KnownCameraControlFlag::Manual => Ok(0),
                        nokhwa::KnownCameraControlFlag::Automatic => Ok(1)
                    }
                }
            }
        } 

        pub fn run_command(&mut self, ev: UVCCmd) {
            match ev {
                UVCCmd::GetCtrlDescr(camctrl, s) => {
                    s.send(self.get_ctrl_descr(camctrl).map(|d| d.clone())).ok();
                },
                UVCCmd::SetCtrl(camctrl, vl, s) => {
                    s.send(self.set_ctrl(camctrl, vl)).ok();
                },
                UVCCmd::GetCtrl(camctrl, s) => {
                    s.send(self.get_ctrl(camctrl)).ok();
                } 
            }
        }
    }
    pub fn run_handler(ncam: u8, send_find: oneshot::Sender<Result<(String,String),UVIError>>,
            mut recv_cmd: mpsc::Receiver<UVCCmd>) {
        thread::spawn(move || {
            match find_camera(ncam) {
                Err(e) => {
                    send_find.send(Err(e)).ok();
                },
                Ok((mut cam, card, bus)) => {
                    send_find.send(Ok((card, bus))).ok();
                    while let Some(ev) = recv_cmd.blocking_recv() {
                        cam.run_command(ev);
                    }
                }
            }
        });
    }
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