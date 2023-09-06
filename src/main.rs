#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::{BTreeMap, HashSet};
use std::net;
use std::sync::Arc;
use crate::auto_uvc::CamCmd;

mod presetdb;
mod viscaip;
mod uvc;
mod auto_uvc;
mod uvierror;
use crate::uvierror::{UVIResult, UVIError};
use std::io::ErrorKind;
#[cfg(all(not(feature="uvcmock"), target_os = "linux"))]
mod uvc_linux;
#[cfg(all(not(feature="uvcmock"), target_os = "windows"))]
mod uvc_win;
#[cfg(feature="uvcmock")]
mod uvc_mock;

use iced::{
    window, executor, Alignment, Element, Application, Command, Settings, Length, Subscription, Theme
};
use iced::widget::{Text, Column};

use tokio::sync::mpsc;
use tokio::task;
use tokio::time::{Duration, Instant, sleep_until};

#[derive(Default, Debug, Clone)]
pub struct CamAppState {
    ncam: u8,
    viscaport: Option<u32>,
    bus: String,
    ncnx: i64,
}

#[derive(Default, Debug)]
struct WebCamViscaIPApp {
    cams: BTreeMap<u8, CamAppState>,
    sender_main_events: Option<mpsc::Sender<MainEvent>>
}

#[derive(Debug, Clone)]
pub enum Message {
    AdminChannelReady(mpsc::Sender<MainEvent>),
    CamerasReady,
    NewCam(u8, String),
    NewViscaCam(u8, u32),
    LostViscaCam(u8),
    NewViscaConnection(u8, net::SocketAddr),
    LostViscaConnection(u8, net::SocketAddr)
}

#[derive(Debug)]
enum AppSubscrState {
    Starting,
    Ready(mpsc::Receiver<MainEvent>)
}

#[derive(Debug)]
pub enum MainEvent {
  NewCam(u8, String),
  NewViscaCam(u8, u32),
  NewViscaConnection(u8, net::SocketAddr),
  LostViscaConnection(u8, net::SocketAddr),
  LostViscaCam(u8)
}

impl Application for WebCamViscaIPApp {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (WebCamViscaIPApp, Command<Self::Message>) {
        (
            WebCamViscaIPApp::default(),
            Command::none()
        )
    }

    fn title(&self) -> String {
        String::from("WebCam Visca IP")
    }

    fn update(&mut self, message: Message) -> Command<Self::Message> {
        //println!("Message: {:?}", message);
        match message {
            Message::AdminChannelReady(sender) => {
                self.sender_main_events = Some(sender.clone());
                Command::perform(start_camera_activation(sender), |_| Message::CamerasReady)
            },
            Message::CamerasReady => {
                Command::none()
            },
            Message::NewCam(ncam, bus) => {
                let cam = CamAppState {
                    ncam: ncam,
                    viscaport: None,
                    bus: bus,
                    ncnx: 0
                };
                self.cams.insert(ncam, cam);
                Command::none()
            },
            Message::NewViscaCam(ncam, port) => {
                let cam = &mut self.cams.get_mut(&ncam).expect("ncam not active");
                cam.viscaport = Some(port);
                Command::none()
            },
            Message::NewViscaConnection(ncam, _addr) => {
                let cam = &mut self.cams.get_mut(&ncam).expect("ncam not active");
                cam.ncnx += 1;
                //println!("Accepted from: {} ncam: {}", addr, ncam);
                Command::none()
            },
            Message::LostViscaConnection(ncam, _addr) => {
                let cam = &mut self.cams.get_mut(&ncam).expect("ncam not active");
                cam.ncnx -= 1;
                //println!("Disconnect from: {} ncam: {}", addr, ncam);
                Command::none()
            },
            Message::LostViscaCam(ncam) => {
                self.cams.remove(&ncam);
                Command::none()
            },
        }
    }
    fn subscription(&self) -> Subscription<Message> {
        iced::subscription::unfold("cam actions", AppSubscrState::Starting, |state| async move {
            match state {
                AppSubscrState::Starting => {
                    let (sender, receiver) = mpsc::channel(100);
                    ((Message::AdminChannelReady(sender)), AppSubscrState::Ready(receiver))
                }
                AppSubscrState::Ready(mut receiver) => {
                        // Read next input sent from `Application`
                    let ev = receiver.recv().await.expect("can't happen: copy of sender retained");
    
                    match ev {
                        MainEvent::NewCam(ncam, bus) => {
                            ((Message::NewCam(ncam, bus)),
                                 AppSubscrState::Ready(receiver))
                        },
                        MainEvent::NewViscaCam(ncam, port) => {
                            ((Message::NewViscaCam(ncam, port)),
                                 AppSubscrState::Ready(receiver))
                        },
                        MainEvent::NewViscaConnection(ncam, addr) => {
                            ((Message::NewViscaConnection(ncam, addr)),
                                 AppSubscrState::Ready(receiver))
                        },
                        MainEvent::LostViscaConnection(ncam, addr) => {
                            ((Message::LostViscaConnection(ncam, addr)),
                                 AppSubscrState::Ready(receiver))
                        },
                        MainEvent::LostViscaCam(ncam) => {
                            ((Message::LostViscaCam(ncam)),
                                 AppSubscrState::Ready(receiver))
                        }
                    }
                }
            }
        })
    }
    fn view(&self) -> Element<Message> {
        let mut col = Column::new()
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_items(Alignment::Start)
            .push(Text::new("List of active VISCA IP WebCams:").size(24));
        for (_ncam, cam) in self.cams.iter() {
            let txt = match cam.viscaport {
                Some(port) => format!("#{} / VISCA port {} / Bus {}: TCP Conections {}", cam.ncam, port, cam.bus, cam.ncnx),
                None => format!("#{} / Regular WebCam / Bus {}", cam.ncam, cam.bus)
            };
            col = col.push(Text::new(txt).size(16));
        }
        col.into()
    }
}

async fn try_to_activate_all_cams(ncams: &mut HashSet<u8>, send_main_event: &mpsc::Sender<MainEvent>,
        send_ncamdead: &mpsc::Sender<u8>) -> UVIResult<()> {
    'nextcamdev: for ncamdev in 0..8 {
        if ncams.contains(&ncamdev.into()) { continue 'nextcamdev; }

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
                let mut port: u32 = 5678 + ncamdev as u32;
                loop {
                    match viscaip::activate_visca_port(port, ncamdev, send_main_event.clone(),
                            cam_chan.clone(), send_ncamdead.clone()).await {
                        Ok(_) => {
                            ncams.insert(ncamdev);
                            send_main_event.send(MainEvent::NewCam(ncamdev, cam.bus.clone())).await.ok();
                            send_main_event.send(MainEvent::NewViscaCam(ncamdev, port)).await.ok();
                            break;
                        },
                        Err(UVIError::IoError(e)) if e.kind() == ErrorKind::AddrInUse => (),
                        Err(error) => {
                            eprintln!("Problem opening tcp port: {:?}", error);
                            continue 'nextcamdev;
                        }
                    }
                    port += 1;
                    if port >= 5700 { 
                        eprintln!("No tcp ports available");
                        continue 'nextcamdev;
                    }
                }
            },
            None => {
                send_main_event.send(MainEvent::NewCam(ncamdev, cam.bus.clone())).await.ok();
                ncams.insert(ncamdev);
            }
        }
    }
    Ok(())
}

async fn continuous_activation_all_cams(send_main_event: mpsc::Sender<MainEvent>) {
    let mut ncams: HashSet<u8> = HashSet::new();
    let (send_ncamdead, mut recv_ncamdead) = mpsc::channel(100);
    loop {
        try_to_activate_all_cams(&mut ncams, &send_main_event, &send_ncamdead).await.unwrap();
        let until = Instant::now() + Duration::from_millis(3000);
        loop {
            tokio::select! {
                _ = sleep_until(until) => {
                    break;
                },
                Some(ncamdead) = recv_ncamdead.recv() => {
                    send_main_event.send(MainEvent::LostViscaCam(ncamdead)).await.ok();
                    ncams.remove(&ncamdead);
                }
            }
        }
    }
}

async fn start_camera_activation(send_main_event: mpsc::Sender<MainEvent>) {
    task::spawn(async move {
        continuous_activation_all_cams(send_main_event).await
    });
}

pub fn main() -> iced::Result {
    presetdb::prepare_preset_db().expect("problem on db file?");
    WebCamViscaIPApp::run(Settings {
        window: window::Settings {
            size: (600,300),
            ..Default::default()
        },
        ..Default::default()
    })
}

