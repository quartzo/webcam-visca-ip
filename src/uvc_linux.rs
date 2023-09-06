use crate::uvierror::{UVIResult, UVIError};
use std::collections::HashMap;
use crate::uvc::{Description, CamControl, ControlType, UVCCmd};
use tokio::sync::{mpsc,oneshot};
use v4l::Device;
use tokio::task;
use v4l::control;
use v4l::capability;
//use std::sync::Mutex;
use once_cell::sync::Lazy;

#[derive(Debug,Clone)]
pub struct DescriptionInt {
    id: u32,
    descr: Description
}

pub struct CamInterno {
    dev: Device,
    ctrls: HashMap<CamControl, DescriptionInt>
}

static CTRLIDMAP: Lazy<HashMap<u32, CamControl>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(0x009a0908, CamControl::PanAbsolute);
    m.insert(0x009a0909, CamControl::TiltAbsolute);
    m.insert(0x009a090d, CamControl::ZoomAbsolute);
    m.insert(0x009a090a, CamControl::FocusAbsolute);
    m.insert(0x009a090c, CamControl::FocusAuto);
    m.insert(0x0098091a, CamControl::WhiteBalanceTemperature);
    m.insert(0x0098090c, CamControl::WhiteBalanceTemperatureAuto);
    m
});

pub async fn find_camera(ncam: u8) -> UVIResult<(CamInterno,String,String)> {
    let dev = Device::new(ncam.into())?;
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
        if let Some(control_e) = CTRLIDMAP.get(&control.id) {
            let typ = match control.typ {
                control::Type::Integer => ControlType::Integer,
                control::Type::Boolean => ControlType::Boolean,
                _ => break
            };
            let descr = DescriptionInt {
                id: control.id,
                descr: Description {
                    typ: typ,
                    minimum: control.minimum,
                    maximum: control.maximum,
                    step: control.step,
                    default: control.default,
                }
            };
            cam.ctrls.insert(*control_e, descr);
        }
    }
    Ok((cam, caps.card, caps.bus))
}

impl CamInterno {
    fn get_ctrl_descr(&self, camctrl: CamControl) -> UVIResult<&DescriptionInt> {
        self.ctrls.get(&camctrl).ok_or(UVIError::CamControlNotFound)
    }
    fn set_ctrl(&self, camctrl: CamControl, vl: i64) -> UVIResult<()> {
        let ctrl = self.get_ctrl_descr(camctrl)?;
        self.dev.set_control(control::Control {
            id: ctrl.id,
            value: control::Value::Integer(vl)
        })?;
        Ok(())
    }
    fn get_ctrl(&self, camctrl: CamControl) -> UVIResult<i64> {
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
                s.send(self.get_ctrl_descr(ctrlname).map(|d| d.descr.clone())).ok();
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
