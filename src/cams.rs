use std::collections::{HashMap};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task;
use std::net;
use tokio::time::{Duration, Instant, sleep_until};

use crate::auto_uvc::CamCmd;
use crate::viscaip;
use crate::uvc;
use crate::auto_uvc;
use crate::uvierror::{UVIResult, UVIError};
use crate::teleport;
use crate::MainEvent;

#[derive(Debug)]
pub enum CamsMsgs {
  NewViscaConnection(u8, net::SocketAddr),
  LostViscaConnection(u8, net::SocketAddr),
  NCamDead(u8)
}

struct CamData {
    ncam: u8,
    viscaport: Option<u32>,
    bus: String,
    ncnx: i64,
    _teleport_chan: mpsc::Sender<()>,
}

struct AllCams {
    ncams: HashMap<u8,CamData>,
    send_main_event: mpsc::Sender<MainEvent>,
    send_cams_msgs: mpsc::Sender<CamsMsgs>,
    recv_cams_msgs: mpsc::Receiver<CamsMsgs>,
}
impl AllCams {
    async fn continuous_activation_all_cams(&mut self) {
        loop {
            self.try_to_activate_all_cams().await.unwrap();
            let until = Instant::now() + Duration::from_millis(3000);
            loop {
                tokio::select! {
                    _ = sleep_until(until) => {
                        break;
                    },
                    Some(msg) = self.recv_cams_msgs.recv() => {
                        match msg {
                            CamsMsgs::NCamDead(ncamdead) => {
                                self.ncams.remove(&ncamdead);
                            },
                            CamsMsgs::NewViscaConnection(ncam, _addr) => {
                                let cam_data = &mut self.ncams.get_mut(&ncam).expect("ncam not active");
                                cam_data.ncnx += 1;
                                //println!("Accepted from: {} ncam: {}", addr, ncam);
                                self.update_screen().await;
                            },
                            CamsMsgs::LostViscaConnection(ncam, _addr) => {
                                let camdata = &mut self.ncams.get_mut(&ncam).expect("ncam not active");
                                camdata.ncnx -= 1;
                                //println!("Disconnect from: {} ncam: {}", addr, ncam);
                                self.update_screen().await;
                            },
                        }
                    }
                }
            }
        }
    }
    async fn try_to_activate_all_cams(&mut self) -> UVIResult<()> {
        for ncamdev in 0..8 {
            if self.ncams.contains_key(&ncamdev.into()) { continue; }
    
            let cam = match uvc::find_camera(ncamdev).await {
                Ok(n) => Ok(Arc::new(n)),
                Err(UVIError::IoError(_)) => continue,
                Err(UVIError::CameraNotFound) => continue,
                #[cfg(target_os = "windows")]
                Err(UVIError::NokhwaError(_)) => continue,
                Err(x) => Err(x)
            }?;
            let o_cam_chan = match auto_uvc::AutoCamera::activate_camera_ctrls(cam.clone()).await {
                Ok(n) => Ok(Some(n)),
                Err(UVIError::IoError(_)) => continue,
                Err(UVIError::CamControlNotFound) => Ok(None),
                #[cfg(target_os = "windows")]
                Err(UVIError::NokhwaError(_)) => continue,
                Err(x) => Err(x)
            }?;
            match o_cam_chan {
                Some(cam_chan) => {
                    cam_chan.send(CamCmd::SetPresetNcam(ncamdev)).ok();
                    let port: u32 = 5678 + ncamdev as u32;
                    match viscaip::activate_visca_port(port, ncamdev,
                            cam_chan.clone(), self.send_cams_msgs.clone()).await {
                        Ok(_) => {
                            self.ncams.insert(ncamdev, CamData{
                                ncam: ncamdev,
                                viscaport: Some(port),
                                bus: cam.bus.clone(),
                                ncnx: 0,
                                _teleport_chan: teleport::announce_teleport(ncamdev).await?
                            });
                            self.update_screen().await;
                        },
                        Err(error) => {
                            eprintln!("Problem opening tcp port: {:?}", error);
                        }
                    }
                },
                None => {
                    self.ncams.insert(ncamdev, CamData{
                        ncam: ncamdev,
                        viscaport: None,
                        bus: cam.bus.clone(),
                        ncnx: 0,
                        _teleport_chan: teleport::announce_teleport(ncamdev).await?
                    });
                    self.update_screen().await;
                }
            }
        }
        Ok(())
    }
    async fn update_screen(&self) {
        let mut col: Vec<String> = Vec::new();
        for (_ncam, cam) in self.ncams.iter() {
            let txt = match cam.viscaport {
                Some(port) => format!("#{} / VISCA port {} / Bus {}: TCP Conections {}", cam.ncam, port, cam.bus, cam.ncnx),
                None => format!("#{} / Regular WebCam / Bus {}", cam.ncam, cam.bus)
            };
            col.push(txt);
        }
        self.send_main_event.send(MainEvent::UpdateScreen(col)).await.ok();
    }
}

pub async fn start_camera_activation(send_main_event: mpsc::Sender<MainEvent>) {
    let (send_cams_msgs, recv_cams_msgs) = mpsc::channel(100);
    let mut allcams = AllCams {
        ncams: HashMap::new(),
        send_main_event,
        send_cams_msgs, recv_cams_msgs,
    };
    task::spawn(async move {
        allcams.continuous_activation_all_cams().await
    });
}

