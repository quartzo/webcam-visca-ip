use std::collections::HashMap;
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
use iced_native::futures::channel::mpsc;
use iced_native::futures::SinkExt;

#[derive(Default, Debug, Clone)]
pub struct CamAppState {
    ncam: u8,
    port: u32,
    bus: String,
    ncnx: i64,
}

#[derive(Default, Debug)]
struct WebCamViscaIPApp {
    cams: HashMap<u8, CamAppState>,
    sender_main_events: Option<mpsc::Sender<protos::MainEvent>>
}

#[derive(Debug, Clone)]
pub enum Message {
    AdminChannelReady(mpsc::Sender<protos::MainEvent>),
    CamerasReady,
    NewViscaCam(u8, u32, String),
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
                Command::perform(initialize_cameras(sender), |_| Message::CamerasReady)
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
                    use iced_native::futures::StreamExt;
                        // Read next input sent from `Application`
                    let ev = receiver.select_next_some().await;
    
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
                        }
                    }
                }
            }
        })
    }
    fn view(&mut self) -> Element<Message> {
        let mut ln: Vec<String> = Vec::new();
        for ncam in 0..16 {
            ln.push(match self.cams.get_mut(&ncam) {
                Some(cam) => {
                    format!("#{} / VISCA port {} / Bus {}: TCP Conections {}", cam.ncam, cam.port, cam.bus, cam.ncnx)
                },
                None => String::new()
            });
        }
        Column::new()
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_items(Alignment::Start)
            .push(Text::new("List of active VISCA IP WebCams:").size(24))
            .push(Text::new(&ln[0]).size(16))
            .push(Text::new(&ln[1]).size(16))
            .push(Text::new(&ln[2]).size(16))
            .push(Text::new(&ln[3]).size(16))
            .push(Text::new(&ln[4]).size(16))
            .push(Text::new(&ln[6]).size(16))
            .push(Text::new(&ln[7]).size(16))
            .push(Text::new(&ln[8]).size(16))
            .into()
    }
}

async fn initialize_cameras(mut send_main_event: mpsc::Sender<protos::MainEvent>) {
    presetdb::prepare_preset_db().await.unwrap();
    let mut ncam: u8 = 0;
    let mut port: u32 = 5678;
    for ncamdev in 0..16 {
        let (cam_chan, bus) = match auto_uvc::AutoCamera::find_camera(ncamdev, ncam).await {
            Ok(n) => Ok(n),
            Err(UVIError::IoError(_)) => continue,
            Err(UVIError::CameraNotFound) => continue,
            Err(UVIError::CamControlNotFound) => continue,
            #[cfg(target_os = "windows")]
            Err(UVIError::NokhwaError(_)) => continue,
            Err(x) => Err(x)
        }.unwrap();
        for _ in 0..10 {
            match viscaip::init(port, ncam, send_main_event.clone(), cam_chan.clone()).await {
                Ok(_) => break,
                Err(UVIError::IoError(e)) if e.kind() == ErrorKind::AddrInUse => port += 1,
                Err(error) => panic!("Problem opening tcp port: {:?}", error)
            }
        }
        send_main_event.send(protos::MainEvent::NewViscaCam(ncam, port, bus)).await.unwrap();
        ncam += 1; port += 1;
    }
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

