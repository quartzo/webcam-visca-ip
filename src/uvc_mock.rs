use crate::uvierror::UVIError;
use std::collections::HashMap;
use std::thread;
use tokio::sync::{mpsc,oneshot};
use crate::uvc::{Description, CamControl, ControlType, UVCCmd};
    
pub struct CamInterno {
    ncam: u8,
    ctrls: HashMap<CamControl, Description>,
    memory: HashMap<CamControl, i32>
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
        ncam,
        ctrls,
        memory: HashMap::new()
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
                println!("{} {} {}", self.ncam, camctrl, vl as i32);
            },
            ControlType::Boolean => {
                self.memory.insert(camctrl, if vl != 0 { 1 } else { 0 });
                println!("{} {} {}", self.ncam, camctrl, if vl != 0 {true} else {false});
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
        match mock_find_camera(ncam) {
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
