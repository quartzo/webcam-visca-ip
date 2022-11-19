use crate::uvierror::UVIError;
use std::collections::HashMap;
use tokio::sync::{mpsc,oneshot};
use tokio::task;
use crate::uvc::{Description, CamControl, ControlType, UVCCmd};
use tokio::time::{Duration, Instant, sleep_until};

pub struct CamInterno {
    ctrls: HashMap<CamControl, Description>,
    memory: HashMap<CamControl, i32>,
    changed: bool
}

pub fn mock_find_camera(ncam: u8) -> Result<(CamInterno, String, String), UVIError> {
    if ncam > 2 {
        return Err(UVIError::CameraNotFound)
    }
    let card = "mock".to_string();
    let bus = format!("#{}", ncam);
    let mut ctrls = HashMap::new();
    ctrls.insert(CamControl::PanAbsolute, Description {
        typ: ControlType::Integer,
        minimum: -612000,
        maximum: 612000,
        step: 1,
        default: 0,
    });
    ctrls.insert(CamControl::TiltAbsolute, Description {
        typ: ControlType::Integer,
        minimum: -108000,
        maximum: 108000,
        step: 1,
        default: 0,
    });
    ctrls.insert(CamControl::ZoomAbsolute, Description {
        typ: ControlType::Integer,
        minimum: 0,
        maximum: 5680,
        step: 1,
        default: 0,
    });
    ctrls.insert(CamControl::FocusAbsolute, Description {
        typ: ControlType::Integer,
        minimum: 0,
        maximum: 2900,
        step: 1,
        default: 0,
    });
    ctrls.insert(CamControl::FocusAuto, Description {
        typ: ControlType::Boolean,
        minimum: 0,
        maximum: 1,
        step: 1,
        default: 1,
    });
    ctrls.insert(CamControl::WhiteBalanceTemperature, Description {
        typ: ControlType::Integer,
        minimum: 2500,
        maximum: 8000,
        step: 1,
        default: 0,
    });
    ctrls.insert(CamControl::WhiteBalanceTemperatureAuto, Description {
        typ: ControlType::Boolean,
        minimum: 0,
        maximum: 1,
        step: 1,
        default: 1,
    });
    let cam = CamInterno {
        ctrls,
        memory: HashMap::new(),
        changed: false
    };
    Ok((cam, card, bus))
}

impl CamInterno {
    pub fn get_ctrl_descr(&self, camctrl: CamControl) -> Result<&Description, UVIError> {
        self.ctrls.get(&camctrl).ok_or(UVIError::CamControlNotFound)
    }
    pub fn set_ctrl(&mut self, camctrl: CamControl, vl: i64) -> Result<(), UVIError> {
        let ctrldescr = self.get_ctrl_descr(camctrl)?;
        match ctrldescr.typ {
            ControlType::Integer => {
                self.memory.insert(camctrl, vl as i32);
                self.changed = true;
            },
            ControlType::Boolean => {
                self.memory.insert(camctrl, if vl != 0 { 1 } else { 0 });
                self.changed = true;
            }
        }
        Ok(())
    } 
    pub fn get_ctrl(&self, camctrl: CamControl) -> Result<i64, UVIError> {
        let ctrldescr = self.get_ctrl_descr(camctrl)?;
        let vl = *self.memory.get(&camctrl).unwrap_or(&0) as i64;
        match ctrldescr.typ {
            ControlType::Integer => {
                Ok(vl)
            },
            ControlType::Boolean => {
                Ok(vl)
            }
        }
    } 

    pub async fn run_command(&mut self, ev: UVCCmd) {
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
    task::spawn(async move {
        match mock_find_camera(ncam) {
            Err(e) => {
                send_find.send(Err(e)).ok();
            },
            Ok((mut cam, card, bus)) => {
                send_find.send(Ok((card, bus))).ok();
                loop {
                    let until = Instant::now() + Duration::from_millis(200);
                    loop {
                        tokio::select! {
                            _ = sleep_until(until) => {
                                break;
                            },       
                            Some(ev) = recv_cmd.recv() => {
                                cam.run_command(ev).await;
                            }
                        }
                    };
                    if cam.changed {
                        cam.changed = false;
                        let pan = cam.get_ctrl(CamControl::PanAbsolute).unwrap();
                        let tilt = cam.get_ctrl(CamControl::TiltAbsolute).unwrap();
                        let zoom = cam.get_ctrl(CamControl::ZoomAbsolute).unwrap();
                        println!("{: <1$}pan{pan:>7} tilt{tilt:>7} zoom{zoom:>7}", "", 
                            (ncam*36) as usize, pan=pan, tilt=tilt, zoom=zoom);
                    }
                }
            }
        }
    });
}
