#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::net;

mod presetdb;
mod protos;
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
    port: u32,
    bus: String,
    ncnx: i64,
}

#[derive(Default, Debug)]
struct WebCamViscaIPApp {
    cams: BTreeMap<u8, CamAppState>,
    sender_main_events: Option<mpsc::Sender<protos::MainEvent>>
}

#[derive(Debug, Clone)]
pub enum Message {
    AdminChannelReady(mpsc::Sender<protos::MainEvent>),
    CamerasReady,
    NewViscaCam(u8, u32, String),
    LostViscaCam(u8),
    NewViscaConnection(u8, net::SocketAddr),
    LostViscaConnection(u8, net::SocketAddr)
}

#[derive(Debug)]
enum AppSubscrState {
    Starting,
    Ready(mpsc::Receiver<protos::MainEvent>)
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
            Message::NewViscaCam(ncam, port, bus) => {
                let cam = CamAppState {
                    ncam: ncam,
                    port: port,
                    bus: bus,
                    ncnx: 0
                };
                self.cams.insert(ncam, cam);
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
                        protos::MainEvent::NewViscaCam(ncam, port, bus) => {
                            ((Message::NewViscaCam(ncam, port, bus)),
                                 AppSubscrState::Ready(receiver))
                        },
                        protos::MainEvent::NewViscaConnection(ncam, addr) => {
                            ((Message::NewViscaConnection(ncam, addr)),
                                 AppSubscrState::Ready(receiver))
                        },
                        protos::MainEvent::LostViscaConnection(ncam, addr) => {
                            ((Message::LostViscaConnection(ncam, addr)),
                                 AppSubscrState::Ready(receiver))
                        },
                        protos::MainEvent::LostViscaCam(ncam) => {
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
            col = col.push(Text::new(
                format!("#{} / VISCA port {} / Bus {}: TCP Conections {}", cam.ncam, cam.port, cam.bus, cam.ncnx)
            ).size(16));
        }
        col.into()
    }
}

struct ActiveCams {
    ncams: BTreeMap<u8, u8> // sequencial detected cams -> oper. system cam
}
impl ActiveCams {
    fn new() -> ActiveCams {
        ActiveCams{ncams:BTreeMap::new()}
    }
    fn cam_dev_already_active(&self, ncamdev: u8) -> bool {
        for (_, ncamdev2) in self.ncams.iter() {
            if ncamdev == *ncamdev2 { return true; }
        }
        false
    }
    fn find_first_cam_free(&self) -> Option<u8> {
        let mut ncam: u8 = 0;
        loop {
            if !self.ncams.contains_key(&ncam) { return Some(ncam); }
            ncam += 1;
            if ncam > 100 { return None }
        }
    }
    fn cam_active(&mut self, ncam: u8, ncamdev: u8) {
        self.ncams.insert(ncam, ncamdev);
    }
    fn cam_dead(&mut self, ncam: u8){
        self.ncams.remove(&ncam);
    }
}

async fn try_to_activate_all_cams(ncams: &mut ActiveCams, send_main_event: &mpsc::Sender<protos::MainEvent>,
        send_ncamdead: &mpsc::Sender<u8>) -> UVIResult<()> {
    'nextcamdev: for ncamdev in 0..8 {
        if ncams.cam_dev_already_active(ncamdev) { continue 'nextcamdev; }
        let (cam_chan, bus) = match auto_uvc::AutoCamera::find_camera(ncamdev).await {
            Ok(n) => Ok(n),
            Err(UVIError::IoError(_)) => continue,
            Err(UVIError::CameraNotFound) => continue,
            Err(UVIError::CamControlNotFound) => continue,
            #[cfg(target_os = "windows")]
            Err(UVIError::NokhwaError(_)) => continue,
            Err(x) => Err(x)
        }?;
        let ncam = if let Some(x) = ncams.find_first_cam_free() { x } else {
            continue 'nextcamdev
        };
        cam_chan.send(protos::CamCmd::SetPresetNcam(ncam)).ok();
        let mut port: u32 = 5678 + ncam as u32;
        loop {
            match viscaip::activate_visca_port(port, ncam, send_main_event.clone(),
                    cam_chan.clone(), send_ncamdead.clone()).await {
                Ok(_) => {
                    ncams.cam_active(ncam, ncamdev);
                    send_main_event.send(protos::MainEvent::NewViscaCam(ncam, port, bus)).await.ok();
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
    }
    Ok(())
}

async fn continuous_activation_all_cams(send_main_event: mpsc::Sender<protos::MainEvent>) {
    let mut ncams = ActiveCams::new();
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
                    send_main_event.send(protos::MainEvent::LostViscaCam(ncamdead)).await.ok();
                    ncams.cam_dead(ncamdead);
                }
            }
        }
    }
}

async fn start_camera_activation(send_main_event: mpsc::Sender<protos::MainEvent>) {
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

