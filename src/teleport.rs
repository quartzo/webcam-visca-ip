use std::net::{Ipv4Addr, UdpSocket, SocketAddr};
use std::str::FromStr;
use tokio::net::{TcpListener, TcpStream};
use once_cell::sync::Lazy;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};
use crate::uvierror::UVIResult;
use serde::Serialize;
use serde_json;
use hostname;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::cams::CamsMsgs;

static HOSTNAME: Lazy<String> = Lazy::new(|| 
    hostname::get().expect("No Hostname?").into_string().expect("Bad string from Hostame")
);

static ANNOUCE_ACCEPTORS_HOST: &str = "239.255.255.250";
static ANNOUCE_ACCEPTORS_ADDR: Lazy<String> = Lazy::new(|| ANNOUCE_ACCEPTORS_HOST.to_owned()+":9999");

#[derive(Serialize, Debug)]
#[allow(non_snake_case)]
struct AnnouncePayload<'a> {
	Name:          &'a str,
	Port:          i32,
	AudioAndVideo: bool,
	Version:       &'a str
}

struct TeleportCamClient {
    sender: mpsc::Sender<Arc<Vec<u8>>>,
}
struct TeleportCam {
    ncam: u8,
    bufs: Mutex<Vec<Arc<TeleportCamClient>>>,
    cams_send: mpsc::Sender<CamsMsgs>,
}
impl TeleportCam {
    fn new(ncam: u8, cams_send: mpsc::Sender<CamsMsgs>) -> Arc<TeleportCam> {
        Arc::new(TeleportCam{ncam, bufs: Mutex::new(Vec::new()), cams_send})
    }
    async fn sender_add(self: &Arc<TeleportCam>, mut socketc: TcpStream, _socketc_addr: SocketAddr) {
        let (sender, mut receiver) = mpsc::channel(800);
        let teleportcamclient = Arc::new(TeleportCamClient{
            sender
        });
        self.bufs.lock().await.push(teleportcamclient.clone());
        self.cams_send.send(CamsMsgs::TeleportNumConnections(self.ncam, self.bufs.lock().await.len())).await.unwrap();
        let teleportcam = self.clone();

        tokio::spawn(async move {
            let mut buffer = [0; 256];
            loop {
                tokio::select! {
                    msg = receiver.recv() => {
                        match msg {
                            Some(msgd) => {
                                match socketc.write_all(&msgd).await {
                                    Ok(_) => (),
                                    Err(_) => break
                                }
                            },
                            _ => {
                                break;
                            }
                        }
                    },
                    r = socketc.read(&mut buffer) => {
                        match r {
                            Ok(n) => {
                                if n == 0 { break; }
                            },
                            Err(_) => break
                        }
                    },
                }
            }
            teleportcam.remove_client(teleportcamclient).await;
        });
    }
    async fn send(self: &Arc<TeleportCam>, msg: Vec<u8>) {
        let msg = Arc::new(msg);
        let bufs = self.bufs.lock().await.clone();
        for teleportcamclient in bufs {
            if teleportcamclient.sender.send(msg.clone()).await.is_err() {
                self.remove_client(teleportcamclient).await;
            }
        }
    }
    async fn remove_client(self: &Arc<TeleportCam>, teleportcamclient: Arc<TeleportCamClient>) {
        self.bufs.lock().await.retain(|x| !Arc::ptr_eq(x,&teleportcamclient));
        self.cams_send.send(CamsMsgs::TeleportNumConnections(self.ncam, self.bufs.lock().await.len())).await.unwrap();
    }
    async fn destroy(self: &Arc<TeleportCam>) {
        self.bufs.lock().await.clear(); // sender out of scope, close channel
    }
}

pub async fn announce_teleport(ncam: u8, cams_sendmsg: mpsc::Sender<CamsMsgs>) -> UVIResult<mpsc::Sender<()>> {
    let listener = TcpListener::bind(format!("0.0.0.0:0")).await?;
    //println!("Listening on {}", listener.local_addr()?);
    let listen_port = listener.local_addr()?.port();

    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.join_multicast_v4(
        &Ipv4Addr::from_str(ANNOUCE_ACCEPTORS_HOST).expect("Bad multicast addr?"),
        &Ipv4Addr::UNSPECIFIED)?;
    let pl = AnnouncePayload{
        Name:&HOSTNAME, Port: listen_port as i32,
        AudioAndVideo: false, Version:&"0.0.0"
    };
    let message = serde_json::to_vec(&pl)?;

    // Crie um canal para sinalizar a interrupção
    let (tx, mut rx) = mpsc::channel::<()>(100);
    tokio::spawn(async move {
        let teleportcam = TeleportCam::new(ncam, cams_sendmsg);
        loop {
            tokio::select! {
                _ = sleep(Duration::from_secs(1)) => { // Envie o pacote multicast
                    if socket.send_to(&message, &*ANNOUCE_ACCEPTORS_ADDR).is_ok() {
                        //println!("Sent multicast message: {:?}", pl);
                        //println!("Sent multicast message: {:?}", message);
                    }
                }
                _ = rx.recv() => { // closed control channel -> break
                    break;
                },
                acc = listener.accept() => {
                    let (socketc, socketc_addr) = acc.expect("Bad accept?");
                    teleportcam.sender_add(socketc, socketc_addr).await;
                }
            }
        }
        teleportcam.destroy().await;
    });
    Ok(tx)
}
