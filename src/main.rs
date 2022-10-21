#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::net;

mod presetdb;
mod protos;
mod viscaip;
mod uvc;
mod auto_uvc;
mod uvierror;
use crate::uvierror::UVIError;
use std::io::ErrorKind;

use iced::{
    window, executor, Alignment, Column, Element, Application, Command, Settings, Text, Length
};
use iced_native::subscription::{self, Subscription};
//use iced_native::futures::channel::mpsc;
use tokio::sync::mpsc;
use tokio::task;
use tokio::time::{sleep, Duration};

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

enum AppSubscrState {
    Starting,
    Ready(mpsc::Receiver<protos::MainEvent>)
}

impl Application for WebCamViscaIPApp {
    type Executor = executor::Default;
    type Message = Message;
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
                let mut cam = &mut self.cams.get_mut(&ncam).expect("ncam not active");
                cam.ncnx += 1;
                //println!("Accepted from: {} ncam: {}", addr, ncam);
                Command::none()
            },
            Message::LostViscaConnection(ncam, _addr) => {
                let mut cam = &mut self.cams.get_mut(&ncam).expect("ncam not active");
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
        struct SomeWorker;
        subscription::unfold(std::any::TypeId::of::<SomeWorker>(), AppSubscrState::Starting, |state| async move {
            match state {
                AppSubscrState::Starting => {
                    let (sender, receiver) = mpsc::channel(100);
                    (Some(Message::AdminChannelReady(sender)), AppSubscrState::Ready(receiver))
                }
                AppSubscrState::Ready(mut receiver) => {
                        // Read next input sent from `Application`
                    let ev = receiver.recv().await.expect("can't happen: copy of sender retained");
    
                    match ev {
                        protos::MainEvent::NewViscaCam(ncam, port, bus) => {
                            (Some(Message::NewViscaCam(ncam, port, bus)),
                                 AppSubscrState::Ready(receiver))
                        },
                        protos::MainEvent::NewViscaConnection(ncam, addr) => {
                            (Some(Message::NewViscaConnection(ncam, addr)),
                                 AppSubscrState::Ready(receiver))
                        },
                        protos::MainEvent::LostViscaConnection(ncam, addr) => {
                            (Some(Message::LostViscaConnection(ncam, addr)),
                                 AppSubscrState::Ready(receiver))
                        },
                        protos::MainEvent::LostViscaCam(ncam) => {
                            (Some(Message::LostViscaCam(ncam)),
                                 AppSubscrState::Ready(receiver))
                        }
                    }
                }
            }
        })
    }
    fn view(&mut self) -> Element<Message> {
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

async fn start_camera_activation(send_main_event: mpsc::Sender<protos::MainEvent>) {
    presetdb::prepare_preset_db().await.expect("problem on db file?");
    task::spawn(async move {
        let mut ncams: BTreeMap<u8, u8> = BTreeMap::new();
        let (send_ncamdead, mut recv_ncamdead) = mpsc::channel(100);
        loop {
            for ncamdev in 0..8 {
                let mut cont = false;
                for (_, ncamdev2) in ncams.iter() {
                    if ncamdev == *ncamdev2 { cont = true; }
                }
                if cont { continue; }
                let mut ncam: u8 = 0;
                for _ in 0..200 {
                    if !ncams.contains_key(&ncam) {
                        break;
                    }
                    ncam += 1;
                }
                let (cam_chan, bus) = match auto_uvc::AutoCamera::find_camera(ncamdev, ncam).await {
                    Ok(n) => Ok(n),
                    Err(UVIError::IoError(_)) => continue,
                    Err(UVIError::CameraNotFound) => continue,
                    Err(UVIError::CamControlNotFound) => continue,
                    #[cfg(target_os = "windows")]
                    Err(UVIError::NokhwaError(_)) => continue,
                    Err(x) => Err(x)
                }.unwrap();
                let mut port: u32 = 5678 + ncam as u32;
                for _ in 0..20 {
                    match viscaip::activate_visca_port(port, ncam, bus.clone(), send_main_event.clone(),
                            cam_chan.clone(), send_ncamdead.clone()).await {
                        Ok(_) => {
                            ncams.insert(ncam, ncamdev);
                            break;
                        },
                        Err(UVIError::IoError(e)) if e.kind() == ErrorKind::AddrInUse => port += 1,
                        Err(error) => panic!("Problem opening tcp port: {:?}", error)
                    }
                }
            }
            sleep(Duration::from_millis(3000)).await;
            while let Some(ncamdead) = recv_ncamdead.try_recv().ok() {
                ncams.remove(&ncamdead);
            }
        }
    });
}

pub fn main() -> iced::Result {
    WebCamViscaIPApp::run(Settings {
        window: window::Settings {
            size: (600,300),
            ..Default::default()
        },
        ..Default::default()
    })
}

