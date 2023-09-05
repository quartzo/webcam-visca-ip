use std::fmt;
use tokio::task;
use tokio::sync::mpsc;
use crate::uvc;
use crate::protos;
use crate::presetdb;
use crate::uvierror::UVIError;
use tokio::time;
use std::time::Duration;

#[derive(Default,Debug)]
struct CamCtrl {
  minimum: i64,
  maximum: i64,
  value: i64,
  _step: u64,
}

impl CamCtrl {
  async fn init(cam: &uvc::Camera, camctrl: uvc::CamControl) -> Result<CamCtrl, UVIError> {
    let ctrl = cam.get_ctrl_descr(camctrl).await?;
    let value = cam.get_ctrl(camctrl).await?;
    Ok(CamCtrl {
      minimum: ctrl.minimum,
      maximum: ctrl.maximum,
      value: value,
      _step: ctrl.step
    })
  }
  fn set(&mut self, newval: i64) {
    if newval > self.maximum {
      self.value = self.maximum;
    } else {
      if newval < self.minimum {
        self.value = self.minimum;
      } else {
        self.value = newval;
      }
    }
  }
}

#[derive(Debug)]
struct PanTilt {
  pan: CamCtrl,
  tilt: CamCtrl,
  panspeed: i64,
  tiltspeed: i64,
}

impl PanTilt {
  async fn init(cam: &uvc::Camera) -> Result<PanTilt, UVIError> {
    Ok(PanTilt {
      pan: CamCtrl::init(&cam, uvc::CamControl::PanAbsolute).await?,
      tilt: CamCtrl::init(&cam, uvc::CamControl::TiltAbsolute).await?,
      panspeed: 0,
      tiltspeed: 0,    
    })
  }
  async fn absolute_move(&mut self, cam: &uvc::Camera, pan:i64, tilt:i64) -> Result<(), UVIError> {
    self.panspeed = 0;
    self.tiltspeed = 0;
    self.pan.set(pan);
    cam.set_ctrl(uvc::CamControl::PanAbsolute, self.pan.value).await?;
    self.tilt.set(tilt);
    cam.set_ctrl(uvc::CamControl::TiltAbsolute, self.tilt.value).await?;
    Ok(())
  }
  async fn relative_move(&mut self, cam: &uvc::Camera, pan_move:i64, tilt_move:i64) -> Result<(), UVIError> {
    self.pan.set(self.pan.value + pan_move);
    if self.pan.value >= self.pan.maximum || self.pan.value <= self.pan.minimum {
      self.panspeed = 0;
    }
    cam.set_ctrl(uvc::CamControl::PanAbsolute, self.pan.value).await?;
    self.tilt.set(self.tilt.value + tilt_move);
    if self.tilt.value >= self.tilt.maximum || self.tilt.value <= self.tilt.minimum {
      self.tiltspeed = 0;
    }
    cam.set_ctrl(uvc::CamControl::TiltAbsolute, self.tilt.value).await?;
    Ok(())
  }
  async fn periodic_move(&mut self, cam: &uvc::Camera) -> Result<(), UVIError> {
    // seconds(degree/3600) per second -> /20 for each 50ms
    let mut pan_move = self.panspeed/20; let mut tilt_move = self.tiltspeed/20;
    if pan_move != 0 || tilt_move != 0 {
      if pan_move != 0 {
        let pan_absolute = cam.get_ctrl(uvc::CamControl::PanAbsolute).await?;
        let pandelta = (pan_absolute-self.pan.value).abs();
        if pandelta > 2*60*60 { pan_move = 0; }; // 2 degrees
      }
      if self.tiltspeed != 0 {
        let tilt_absolute = cam.get_ctrl(uvc::CamControl::TiltAbsolute).await?;
        let tiltdelta = (tilt_absolute-self.tilt.value).abs();
        if tiltdelta > 2*60*60 { tilt_move = 0; }; // 2 degrees
      }
      self.relative_move(&cam, pan_move, tilt_move).await?;
    }
    Ok(())
  }
}

#[derive(Debug)]
struct Zoom {
  zoom: CamCtrl,
  zoomspeed: i64,
}

impl Zoom {
  async fn init(cam: &uvc::Camera) -> Result<Zoom, UVIError> {
    Ok(Zoom {
      zoom: CamCtrl::init(&cam, uvc::CamControl::ZoomAbsolute).await?,
      zoomspeed: 0,
    })
  }
  async fn absolute(&mut self, cam: &uvc::Camera, zoom:i64) -> Result<(), UVIError> {
    self.zoomspeed = 0;
    self.zoom.set(zoom);
    cam.set_ctrl(uvc::CamControl::ZoomAbsolute, self.zoom.value).await?;
    Ok(())
  }
  async fn periodic_move(&mut self, cam: &uvc::Camera) -> Result<(), UVIError> {
    if self.zoomspeed != 0 {
      let zoom_absolute = cam.get_ctrl(uvc::CamControl::ZoomAbsolute).await?;
      let zoomdelta = (zoom_absolute-self.zoom.value).abs();
      if zoomdelta < (self.zoom.maximum-self.zoom.minimum)/10 {
        self.zoom.set(self.zoom.value + self.zoomspeed);
        if self.zoom.value >= self.zoom.maximum || self.zoom.value <= self.zoom.minimum {
          self.zoomspeed = 0;
        }
        cam.set_ctrl(uvc::CamControl::ZoomAbsolute, self.zoom.value).await?;
      }
    }
    Ok(())
  }
}

#[derive(Debug)]
struct Focus {
  auto: CamCtrl,
  focus: CamCtrl,
  focusspeed: i64,
}

impl Focus {
  async fn init(cam: &uvc::Camera) -> Result<Focus, UVIError> {
    Ok(Focus {
      auto: CamCtrl::init(&cam, uvc::CamControl::FocusAuto).await?,
      focus: CamCtrl::init(&cam, uvc::CamControl::FocusAbsolute).await?,
      focusspeed: 0
    })
  }
  async fn absolute(&mut self, cam: &uvc::Camera, auto: bool, focus:i64) -> Result<(), UVIError> {
    self.focusspeed = 0;
    self.auto.set(if auto {1} else {0});
    self.focus.set(focus);
    cam.set_ctrl(uvc::CamControl::FocusAuto, self.auto.value).await?;
    if !auto {
      cam.set_ctrl(uvc::CamControl::FocusAbsolute, self.focus.value).await?;
    }
    Ok(())
  }
  async fn periodic_move(&mut self, cam: &uvc::Camera) -> Result<(), UVIError> {
    if self.focusspeed != 0 {
      let focus_absolute = cam.get_ctrl(uvc::CamControl::FocusAbsolute).await?;
      let focusdelta = (focus_absolute-self.focus.value).abs();
      if focusdelta < (self.focus.maximum-self.focus.minimum)/10 {
        self.focus.set(self.focus.value + self.focusspeed);
        if self.focus.value >= self.focus.maximum || self.focus.value <= self.focus.minimum {
          self.focusspeed = 0;
        }
        cam.set_ctrl(uvc::CamControl::FocusAbsolute, self.focus.value).await?;
      }
    }
    Ok(())
  }
}

#[derive(Debug)]
struct WhiteBal {
  auto: CamCtrl,
  temp: CamCtrl
}

impl WhiteBal {
  async fn init(cam: &uvc::Camera) -> Result<WhiteBal, UVIError> {
    Ok(WhiteBal {
      auto: CamCtrl::init(&cam, uvc::CamControl::WhiteBalanceTemperatureAuto).await?,
      temp: CamCtrl::init(&cam, uvc::CamControl::WhiteBalanceTemperature).await?
    })
  }
  async fn absolute(&mut self, cam: &uvc::Camera, auto: bool, temp:i64) -> Result<(), UVIError> {
    self.auto.set(if auto {1} else {0});
    self.temp.set(temp);
    cam.set_ctrl(uvc::CamControl::WhiteBalanceTemperatureAuto, self.auto.value).await?;
    if !auto {
      cam.set_ctrl(uvc::CamControl::WhiteBalanceTemperature, self.temp.value).await?;
    }
    Ok(())
  }
}

#[derive(Debug)]
pub struct AutoCamera {
  cam: uvc::Camera,
  presetdb: Option<presetdb::PresetDB>,
  pantilt: PanTilt,
  zoom: Zoom,
  focus: Focus,
  whitebal: WhiteBal,
}

impl fmt::Display for AutoCamera {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      write!(f, "-Cam {} Pos:{},{},{},{}-", self.cam, self.pantilt.pan.value, 
        self.pantilt.tilt.value, self.zoom.zoom.value, self.focus.focus.value)
  }
}

#[derive(Debug)]
pub struct Preset {
  pub pan: i64, pub tilt: i64, pub zoom: i64,
  pub focusauto: bool, pub focus: i64, 
  pub whitebalauto: bool, pub temperature: i64
}

impl AutoCamera {
  pub async fn find_camera(ndev: u8) -> Result<(mpsc::UnboundedSender<protos::CamCmd>,String), UVIError> {
    let cam = uvc::find_camera(ndev).await?;
    let pantilt = PanTilt::init(&cam).await?;
    let zoom = Zoom::init(&cam).await?;
    let focus = Focus::init(&cam).await?;
    let whitebal = WhiteBal::init(&cam).await?;
    let (cam_chan, recv_cam_chan) = mpsc::unbounded_channel();
    let bus = cam.bus.to_string();
    let acam = AutoCamera {
      cam: cam,
      presetdb: None,
      pantilt: pantilt,
      zoom: zoom,
      focus: focus,
      whitebal: whitebal,
    };
    task::spawn(acam.run(recv_cam_chan));
    Ok((cam_chan,bus))
  }
  async fn run(mut self, mut recv_cam_chan: mpsc::UnboundedReceiver<protos::CamCmd>) {
    let mut tmr50ms = time::interval(Duration::from_millis(50));
    loop {
      tokio::select! {
        _ = tmr50ms.tick() => { // ignore errors here... too noisy
          self.pantilt.periodic_move(&self.cam).await.ok();
          self.zoom.periodic_move(&self.cam).await.ok();
          self.focus.periodic_move(&self.cam).await.ok();
        },
        Some(ev) = recv_cam_chan.recv() => {
          //println!("Ev: {:?}", ev);
          match self.run_ev(ev).await {
            Err(e) => {
              eprintln!("auto_uvc run err: {:?}", e);
              break;
            },
            Ok(run) if run==false => break,
            _ => ()
          }              
        },
        else => break
      }
    }
  }
  async fn run_ev(&mut self, ev: protos::CamCmd) -> Result<bool,UVIError> {
    match ev {
      protos::CamCmd::SetPresetNcam(ncam) => {
        self.presetdb = Some(presetdb::connect_preset_db(ncam)?);
      },
      protos::CamCmd::ResetPreset(npreset) => {
        self.presetdb.as_ref().ok_or(UVIError::CameraNotFound)?.clear(npreset)?;
      },
      protos::CamCmd::RecordPreset(npreset) => {
        let preset = Preset {
          pan: self.pantilt.pan.value,
          tilt: self.pantilt.tilt.value,
          zoom: self.zoom.zoom.value,
          focusauto: if self.focus.auto.value > 0 {true} else {false},
          focus: self.focus.focus.value,
          whitebalauto: if self.whitebal.auto.value > 0 {true} else {false},
          temperature: self.whitebal.temp.value
        };
        self.presetdb.as_ref().ok_or(UVIError::CameraNotFound)?.record(npreset, preset)?;
      },
      protos::CamCmd::RecoverPreset(npreset) => {
        let opreset = self.presetdb.as_ref().ok_or(UVIError::CameraNotFound)?.recover(npreset)?;
        match opreset {
          Some(preset) => {
            self.pantilt.absolute_move(&self.cam, preset.pan, preset.tilt).await?;
            self.zoom.absolute(&self.cam, preset.zoom).await?;
            self.focus.absolute(&self.cam, preset.focusauto, preset.focus).await?;
            self.whitebal.absolute(&self.cam, preset.whitebalauto, preset.temperature).await?;
          },
          _ => ()
        }
      },
      protos::CamCmd::Home() => {
        self.pantilt.absolute_move(&self.cam, 0, 0).await?;
        self.zoom.absolute(&self.cam, self.zoom.zoom.minimum).await?;
        self.focus.absolute(&self.cam, true, self.focus.focus.value).await?;
      },
      protos::CamCmd::MoveContinuous(pantilt) => {
        self.pantilt.panspeed = pantilt.pan; self.pantilt.tiltspeed = pantilt.tilt;
        self.pantilt.periodic_move(&self.cam).await?;
      },
      protos::CamCmd::MoveRelative(pantilt) => {
        self.pantilt.relative_move(&self.cam, pantilt.pan, pantilt.tilt).await?;
      },
      protos::CamCmd::MoveAbsolute(pantilt) => {
        self.pantilt.absolute_move(&self.cam, pantilt.pan, pantilt.tilt).await?;
      },


      protos::CamCmd::ZoomContinuous(zoom_f64) => { // -1 to 1
        self.zoom.zoomspeed = (((self.zoom.zoom.maximum-self.zoom.zoom.minimum) as f64)*
          zoom_f64/20.0) as i64; // ops per 50ms
        self.zoom.periodic_move(&self.cam).await?;
      },
      protos::CamCmd::ZoomDirect(zoom_f64) => { // 0 to 1.0
        self.zoom.absolute(&self.cam, self.zoom.zoom.minimum+
          (((self.zoom.zoom.maximum-self.zoom.zoom.minimum) as f64)*zoom_f64) as i64).await?;
      },
      protos::CamCmd::AutoFocus(active) => {
        self.focus.absolute(&self.cam, active, self.focus.focus.value).await?;
      },
      protos::CamCmd::AutoFocusToggle() => {
        let active = if self.focus.auto.value == 0 {true} else {false};
        self.focus.absolute(&self.cam, active, self.focus.focus.value).await?;
      },
      protos::CamCmd::FocusContinuous(focus_f64) => { // -1 to 1
        self.focus.absolute(&self.cam, false, self.focus.focus.value).await?;
        self.focus.focusspeed = (((self.focus.focus.maximum-self.focus.focus.minimum) as f64)*
          focus_f64/20.0) as i64; // ops per 50ms
        self.focus.periodic_move(&self.cam).await?;
      },
      protos::CamCmd::FocusDirect(focus_f64) => { // 1.0 (Near) - 0.0 (Far)
        self.focus.absolute(&self.cam, true, self.focus.focus.minimum+
          (((self.focus.focus.maximum-self.focus.focus.minimum) as f64)*focus_f64) as i64).await?;
      },
      protos::CamCmd::FocusOnePushTrigger() => {
        // couldn't make it work
      },
      protos::CamCmd::WhiteBalanceTrigger() => {
        // couldn't make it work
      },
      protos::CamCmd::WhiteBalanceMode(wb) => {
        if wb == 0 {       // 0-Auto
          self.whitebal.absolute(&self.cam, true, 6500).await?;
        } else if wb == 1 {       // 1-Indoor
          self.whitebal.absolute(&self.cam, false, 3200).await?;
        } else if wb == 2 {       // 2-Outdoor
          self.whitebal.absolute(&self.cam, false, 5800).await?;
        }
        // 3-One Push WB
        // 4-Auto Tracing
        // 5-Manual  
      },

      protos::CamCmd::QueryPanTilt(s) => {
        s.send(protos::PanTilt {
          pan: self.pantilt.pan.value,
          tilt: self.pantilt.tilt.value,
        }).map_err(|_x| UVIError::AsyncChannelClosed)?;
      },
      protos::CamCmd::QueryFocusMode(s) => {
        s.send(
          if self.focus.auto.value>0 {true} else {false}
        ).map_err(|_x| UVIError::AsyncChannelClosed)?;
      },
      protos::CamCmd::QueryWhiteBalanceMode(s) => {
        s.send(
          if self.whitebal.auto.value > 0 { 0 }
          else if self.whitebal.temp.value < 4000 { 1 }
          else {2}
        ).map_err(|_x| UVIError::AsyncChannelClosed)?;
      },

      /*protos::CamCmd::Close() => {
        return Ok(false)
      }*/
    }
    Ok(true)
  }
}

