use crate::uvierror::UVIError;
use std::collections::HashMap;
use std::thread;
use crate::uvc::{Description, CamControl, ControlType, UVCCmd};
use nokhwa;
use nokhwa::KnownCameraControls::*;
use tokio::sync::{mpsc,oneshot};

#[derive(Debug,Clone)]
pub struct DescriptionInt {
    pub kcontrol: nokhwa::KnownCameraControls,
    sec2degree: bool,
    descr: Description
}

pub struct CamInterno {
    dev: nokhwa::Camera,
    ctrls: HashMap<CamControl, DescriptionInt>
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
                ctrls.insert(CamControl::PanAbsolute, DescriptionInt {
                    kcontrol: *kcontrol,
                    sec2degree: true,
                    descr: Description {
                        typ: ControlType::Integer,
                        minimum: (control.minimum_value()*3600).into(),
                        maximum: (control.maximum_value()*3600).into(),
                        step: control.step() as u64,
                        default: control.default().into(),
                    }
                });
            },
            Tilt => {
                ctrls.insert(CamControl::TiltAbsolute, DescriptionInt {
                    kcontrol: *kcontrol,
                    sec2degree: true,
                    descr: Description {
                        typ: ControlType::Integer,
                        minimum: (control.minimum_value()*3600).into(),
                        maximum: (control.maximum_value()*3600).into(),
                        step: control.step() as u64,
                        default: control.default().into(),
                    }
                });
            },
            Zoom => {
                ctrls.insert(CamControl::ZoomAbsolute, DescriptionInt {
                    kcontrol: *kcontrol,
                    sec2degree: false,
                    descr: Description {
                        typ: ControlType::Integer,
                        minimum: control.minimum_value().into(),
                        maximum: control.maximum_value().into(),
                        step: control.step() as u64,
                        default: control.default().into(),
                    }
                });
            },
            Focus => {
                ctrls.insert(CamControl::FocusAbsolute, DescriptionInt {
                    kcontrol: *kcontrol,
                    sec2degree: false,
                    descr: Description {
                        typ: ControlType::Integer,
                        minimum: control.minimum_value().into(),
                        maximum: control.maximum_value().into(),
                        step: control.step() as u64,
                        default: control.default().into(),
                    }
                });
                ctrls.insert(CamControl::FocusAuto, DescriptionInt {
                    kcontrol: *kcontrol,
                    sec2degree: false,
                    descr: Description {
                        typ: ControlType::Boolean,
                        minimum: 0,
                        maximum: 1,
                        step: 1,
                        default: 1,
                    }
                });
            },        
            WhiteBalance => {
                ctrls.insert(CamControl::WhiteBalanceTemperature, DescriptionInt {
                    kcontrol: *kcontrol,
                    sec2degree: false,
                    descr: Description {
                        typ: ControlType::Integer,
                        minimum: control.minimum_value().into(),
                        maximum: control.maximum_value().into(),
                        step: control.step() as u64,
                        default: control.default().into(),
                    }
                });
                ctrls.insert(CamControl::WhiteBalanceTemperatureAuto, DescriptionInt {
                    kcontrol: *kcontrol,
                    sec2degree: false,
                    descr: Description {
                        typ: ControlType::Boolean,
                        minimum: 0,
                        maximum: 1,
                        step: 1,
                        default: 1,
                    }
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
    pub fn get_ctrl_descr(&self, camctrl: CamControl) -> Result<&DescriptionInt, UVIError> {
        self.ctrls.get(&camctrl).ok_or(UVIError::CamControlNotFound)
    }
    pub fn set_ctrl(&mut self, camctrl: CamControl, mut vl: i64) -> Result<(), UVIError> {
        let ctrldescr = self.get_ctrl_descr(camctrl)?;
        let mut ctrl = self.dev.camera_control(ctrldescr.kcontrol)?;
        match ctrldescr.descr.typ {
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
        match ctrldescr.descr.typ {
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
                s.send(self.get_ctrl_descr(camctrl).map(|d| d.descr.clone())).ok();
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
