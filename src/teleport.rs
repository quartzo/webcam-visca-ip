use std::net::{Ipv4Addr, UdpSocket};
use std::str::FromStr;
use once_cell::sync::Lazy;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};
use crate::uvierror::UVIResult;
use serde::Serialize;
use serde_json;
use hostname;

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

pub async fn announce_teleport(ncam: u8) -> UVIResult<mpsc::Sender<()>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.join_multicast_v4(
        &Ipv4Addr::from_str(ANNOUCE_ACCEPTORS_HOST).expect("Bad multicast addr?"),
        &Ipv4Addr::UNSPECIFIED)?;
    let pl = AnnouncePayload{
        Name:&HOSTNAME, Port:5778+ncam as i32,
        AudioAndVideo: false, Version:&"0.0.0"
    };
    let message = serde_json::to_vec(&pl)?;

    // Crie um canal para sinalizar a interrupção
    let (tx, mut rx) = mpsc::channel::<()>(100);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = sleep(Duration::from_secs(1)) => { // Envie o pacote multicast
                    if socket.send_to(&message, &*ANNOUCE_ACCEPTORS_ADDR).is_ok() {
                        //println!("Sent multicast message: {:?}", pl);
                        //println!("Sent multicast message: {:?}", message);
                    }
                }
                _ = rx.recv() => { // Recebemos um sinal para interromper
                    break;
                }
            }
        }
    });
    Ok(tx)
}
