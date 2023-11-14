use std::net::{Ipv4Addr, UdpSocket, SocketAddr};
use std::str::FromStr;
use tokio::net::{TcpListener, TcpStream};
use once_cell::sync::Lazy;
use tokio::sync::mpsc;
use crate::uvierror::{UVIResult};
use serde::Serialize;
use serde_json;
use hostname;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::cams::CamsMsgs;
use tokio::time::{Duration, sleep, Instant, sleep_until};
use crate::jpeg_fix;

//use v4l::buffer::Type;
//use v4l::io::traits::CaptureStream;
//use v4l::prelude::*;
//use v4l::video::Capture;
//use v4l::{framesize, frameinterval};

use nokhwa::{
    //nokhwa_initialize,
    utils::{CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType},
    Camera,
};


/*
static BOOT_TIME: Lazy<Instant> = Lazy::new(|| Instant::now());
fn os_gettime_ns() -> u64 {
    Instant::now().duration_since(*BOOT_TIME).as_nanos().try_into().unwrap()
}
*/

//use std::fs::File;
//use std::io::prelude::*;

static HOSTNAME: Lazy<String> = Lazy::new(|| 
    hostname::get().expect("No Hostname?").into_string().expect("Bad string from Hostame")
);

static COLOR_RANGE_MIN: [f32; 3] = [0.,0.,0.];
static COLOR_RANGE_MAX: [f32; 3] = [1.,1.,1.];
static COLOR_RANGE_MIN_BYTES: Lazy<Vec<u8>> = Lazy::new(|| {
    let mut r = Vec::new();
    for i in 0..3 {
        r.extend_from_slice(&COLOR_RANGE_MIN[i].to_le_bytes());
    }
    r
});
static COLOR_RANGE_MAX_BYTES: Lazy<Vec<u8>> = Lazy::new(|| {
    let mut r = Vec::new();
    for i in 0..3 {
        r.extend_from_slice(&COLOR_RANGE_MAX[i].to_le_bytes());
    }
    r
});
static COLOR_MATRIX_BYTES: Lazy<Vec<u8>> = Lazy::new(|| {
    //VIDEO_CS_709
    let kb: f32 = 0.0722;
    let kr: f32 = 0.2126;
    let kg = 1. - kb - kr;

    struct Vec3 {
        x: f32, y: f32, z: f32
    }
    struct Matrix3 {
        x: Vec3, y: Vec3, z: Vec3
    }
    fn vec3_dot(v1: &Vec3, v2: &Vec3) -> f32 {
        v1.x*v2.x + v1.y*v2.y + v1.z*v2.z
    }
    fn vec3_rotate(v: &Vec3, m: &Matrix3) -> Vec3 {
        Vec3{
            x: vec3_dot(&v, &m.x),
            y: vec3_dot(&v, &m.y),
            z: vec3_dot(&v, &m.z)
        }
    }

    let min_value:f32 = 16.;
    let max_luma:f32 = 235.;
	let max_chroma:f32 = 240.;
    let mid_chroma:f32 = 0.5 * (min_value + max_chroma);
    let range_min: [f32; 3] = [min_value,min_value,min_value];
    let range_max: [f32; 3] = [max_luma,max_chroma,max_chroma];
    let black_levels: [f32; 3] = [0., mid_chroma, mid_chroma];

	let yvals: f32 = range_max[0] - range_min[0];
	let uvals: f32 = (range_max[1] - range_min[1]) / 2.;
	let vvals: f32 = (range_max[2] - range_min[2]) / 2.;

    let bit_range_max: f32 = 256. - 1.;
	let yscale: f32 = bit_range_max / yvals;
	let uscale: f32 = bit_range_max / uvals;
	let vscale: f32 = bit_range_max / vvals;

    let color_matrix: Matrix3 = Matrix3 {
        x: Vec3{x:yscale, y:0., z:vscale * (1. - kr)},
        y: Vec3{x:yscale, y:uscale * (kb - 1.) * kb / kg, z:vscale * (kr - 1.) * kr / kg},
        z: Vec3{x:yscale, y:uscale * (1. - kb), z:0.}
    };

	let offsets: Vec3 = Vec3{x:-black_levels[0] / bit_range_max,
		 y:-black_levels[1] / bit_range_max,
		 z:-black_levels[2] / bit_range_max};
    let multiplied: Vec3 = vec3_rotate(&offsets, &color_matrix);

    let mut matrix:[f32; 16] = [0.; 16];
    matrix[0] = color_matrix.x.x;
	matrix[1] = color_matrix.x.y;
	matrix[2] = color_matrix.x.z;
	matrix[3] = multiplied.x;

	matrix[4] = color_matrix.y.x;
	matrix[5] = color_matrix.y.y;
	matrix[6] = color_matrix.y.z;
	matrix[7] = multiplied.y;

	matrix[8] = color_matrix.z.x;
	matrix[9] = color_matrix.z.y;
	matrix[10] = color_matrix.z.z;
	matrix[11] = multiplied.z;

	matrix[12] = 0.; matrix[13] = 0.; matrix[14] = 0.;
	matrix[15] = 1.;

    let mut r = Vec::new();
    for i in 0..16 {
        r.extend_from_slice(&matrix[i].to_le_bytes());
    }
    //println!("color matrix {:?}", matrix);
    r
});

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
    socketc_addr: SocketAddr,
    sender: mpsc::Sender<Arc<Vec<u8>>>,
    buffer_full: Mutex<bool>
}
struct TeleportCam {
    ncam: u8,
    bufs: Mutex<Vec<Arc<TeleportCamClient>>>,
    capture_chan: mpsc::Sender<usize>,
    cams_send: mpsc::Sender<CamsMsgs>,
}
impl TeleportCam {
    fn new(ncam: u8, cams_send: mpsc::Sender<CamsMsgs>) -> Arc<TeleportCam> {
        let (capture_chan_sender, capture_chan_receiver) = mpsc::channel(100);
        let teleportcam = Arc::new(TeleportCam{ncam, bufs: Mutex::new(Vec::new()), 
            capture_chan: capture_chan_sender, cams_send});
        teleportcam.start_capturing(capture_chan_receiver);
        teleportcam
    }
    async fn sender_add(self: &Arc<TeleportCam>, mut socketc: TcpStream, socketc_addr: SocketAddr) -> UVIResult<()> {
        let (sender, mut receiver) = mpsc::channel(800);
        let teleportcamclient = Arc::new(TeleportCamClient{
            socketc_addr: socketc_addr.clone(), sender, buffer_full: Mutex::new(false)
        });
        self.bufs.lock().await.push(teleportcamclient.clone());
        self.update_num_clients().await?;
        let teleportcam = self.clone();

        tokio::spawn(async move {
            let mut until = Instant::now() + Duration::from_millis(200);
            loop {
                let mut buffer = [0; 256];
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
                    _ = sleep_until(until) => {
                        until = Instant::now() + Duration::from_millis(200);
                        if *teleportcamclient.buffer_full.lock().await == true {
                            break;
                        }
                    },
                }
            }
            teleportcam.remove_client(socketc_addr).await.unwrap();
        });
        Ok(())
    }
    fn send(self: &Arc<TeleportCam>, msg: Vec<u8>) -> UVIResult<()> {
        let msg = Arc::new(msg);
        let bufs = self.bufs.blocking_lock().clone();
        for teleportcamclient in bufs {
            if teleportcamclient.sender.blocking_send(msg.clone()).is_err() {
                // buffer full, is the connection hanging?
                *teleportcamclient.buffer_full.blocking_lock() = true;
            }
        }
        Ok(())
    }
    async fn remove_client(self: &Arc<TeleportCam>, socketc_addr: SocketAddr) -> UVIResult<()> {
        self.bufs.lock().await.retain(|x| x.socketc_addr != socketc_addr);
        self.update_num_clients().await?;
        Ok(())
    }
    async fn destroy(self: &Arc<TeleportCam>) {
        self.bufs.lock().await.clear(); // sender out of scope, close channel
    }
    async fn update_num_clients(self: &Arc<TeleportCam>) -> UVIResult<()> {
        let ncli = self.bufs.lock().await.len();
        self.cams_send.send(CamsMsgs::TeleportNumConnections(self.ncam, ncli)).await?;
        self.capture_chan.send(ncli).await?;
        Ok(())
    }
    fn activate_camera(self: &Arc<TeleportCam>, mut camch: mpsc::Receiver<()>) -> UVIResult<()> {

        //let cameras = query(ApiBackend::Auto).unwrap();
        //let camera = cameras[self.ncam];

        let index = CameraIndex::Index(self.ncam.into());
        let requested = RequestedFormat::with_formats(RequestedFormatType::HighestFrameRate(30), 
            &[FrameFormat::MJPEG]);
        // make the camera
        println!("Requested format:\n{}", requested);
        let mut camera = Camera::new(index, requested).unwrap();
        println!("Requested format:\n{}", camera.camera_format());

        /*
        // Allocate 4 buffers by default
        let buffer_count = 4;
   
        let fourcc_mjpg = v4l::FourCC::new(b"MJPG");
        let dev = Device::new(self.ncam.into())?;
        let mut framesizes = dev.enum_framesizes(fourcc_mjpg)?;
        let mut width = 1; let mut height = 1;
        while let Some(fs2) = framesizes.pop() {
            match fs2.size {
                framesize::FrameSizeEnum::Discrete(s) => {
                    let mut frameintervals = dev.enum_frameintervals(fourcc_mjpg, s.width, s.height)?;
                    while let Some(fi2) = frameintervals.pop() {
                        match fi2.interval {
                            frameinterval::FrameIntervalEnum::Discrete(i) => {
                                if i.numerator == 1 && i.denominator == 30 {
                                    if s.width*s.height > width*height {
                                        width = s.width; height = s.height;
                                    }
                                }
                            },
                            _ => {}
                        }
                    }
                },
                _ => {}
            }
        }
        let mut format = dev.format()?;
        format.fourcc = fourcc_mjpg;
        format.width = width; format.height = height;
        format = dev.set_format(&format)?;
        let mut params = dev.params()?;
        params.interval.numerator = 1; params.interval.denominator = 30;
        params = dev.set_params(&params)?;

        println!("Active format:\n{}", format);
        println!("Active parameters:\n{}", params);
    
        // Setup a buffer stream and grab a frame, then print its data
        let mut stream = MmapStream::with_buffers(&dev, Type::VideoCapture, buffer_count)?;
    
        // warmup
        stream.next()?;
        */

        camera.open_stream().unwrap();
        let tstamp = Instant::now();
        loop {
            match camch.try_recv() {
                Err(mpsc::error::TryRecvError::Disconnected) => break,
                _ => ()
            }
            
            //let (buf, meta) = stream.next()?;
            let buf = camera.frame_raw().unwrap();
            let good_jpeg = jpeg_fix::get_good_jpeg(buf.as_ref())?;
        
            /*
            println!("Buffer");
            println!("  sequence  : {}", meta.sequence);
            println!("  timestamp : {}", meta.timestamp);
            println!("  flags     : {}", meta.flags);
            println!("  length    : {}", buf.len());
            */

            //let timestamp: u64 = (meta.timestamp.sec as u64)*1000000000+(meta.timestamp.usec as u64)*1000;
            let timestamp: u64 = tstamp.elapsed().as_nanos() as u64;
            let size: i32 = good_jpeg.len() as i32;
            let mut r:Vec<u8> = Vec::new();
       
            // header
            r.extend_from_slice(b"JPEG");
            r.extend_from_slice(&timestamp.to_le_bytes());
            r.extend_from_slice(&size.to_le_bytes());
            // ImageHeader
            r.extend_from_slice(&COLOR_MATRIX_BYTES);
            r.extend_from_slice(&COLOR_RANGE_MIN_BYTES);
            r.extend_from_slice(&COLOR_RANGE_MAX_BYTES);
            // data

            //let mut file = File::create("/tmp/img.jpg")?;
            //file.write_all(&good_jpeg)?;
        
            r.extend_from_slice(&good_jpeg);

            self.send(r.into())?;
        }
        Ok(())
    }

    fn start_capturing(self: &Arc<TeleportCam>, mut capture_chan: mpsc::Receiver<usize>) {
        let teleportcam = self.clone();
        tokio::spawn(async move {
            let mut camch: Option<mpsc::Sender<()>> = None;
            loop {
                match capture_chan.recv().await {
                    None => break,
                    Some(nclient) => {
                        if nclient > 0 && camch.is_none() {
                            let (sender, receiver) = mpsc::channel(800);
                            camch = Some(sender);
                            let teleportcam = teleportcam.clone();
                            tokio::task::spawn_blocking(move || {
                                teleportcam.activate_camera(receiver).unwrap();
                            });
                        }
                        if nclient == 0 {
                            camch = None;
                        }
                    }
                }
            }
        });
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
        Name:&format!("{} #{}", &*HOSTNAME, ncam), Port: listen_port as i32,
        AudioAndVideo: false, Version:&"0.6.6"
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
                    teleportcam.sender_add(socketc, socketc_addr).await.unwrap();
                }
            }
        }
        teleportcam.destroy().await;
    });
    Ok(tx)
}
