#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod presetdb;
mod viscaip;
mod uvc;
mod auto_uvc;
mod uvierror;
#[cfg(all(not(feature="uvcmock"), target_os = "linux"))]
mod uvc_linux;
#[cfg(all(not(feature="uvcmock"), target_os = "windows"))]
mod uvc_win;
#[cfg(feature="uvcmock")]
mod uvc_mock;
mod cams;
mod teleport;

use iced::{
    window, executor, Alignment, Element, Application, Command, Settings, Length, Subscription, Theme
};
use iced::widget::{Text, Column};

use tokio::sync::mpsc;

#[derive(Default, Debug)]
struct WebCamViscaIPApp {
    cams_screen: Vec<String>,
    sender_main_events: Option<mpsc::Sender<MainEvent>>
}

#[derive(Debug, Clone)]
pub enum Message {
    AdminChannelReady(mpsc::Sender<MainEvent>),
    CamerasReady,
    UpdateScreen(Vec<String>)
}

#[derive(Debug)]
enum AppSubscrState {
    Starting,
    Ready(mpsc::Receiver<MainEvent>)
}

#[derive(Debug)]
pub enum MainEvent {
    UpdateScreen(Vec<String>)
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
                Command::perform(cams::start_camera_activation(sender), |_| Message::CamerasReady)
            },
            Message::CamerasReady => {
                Command::none()
            },
            Message::UpdateScreen(lines) => {
                self.cams_screen = lines;
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
                        MainEvent::UpdateScreen(lines) => {
                            ((Message::UpdateScreen(lines)),
                                 AppSubscrState::Ready(receiver))
                        },
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
        for txt in self.cams_screen.iter() {
            col = col.push(Text::new(txt).size(16));
        }
        col.into()
    }
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

