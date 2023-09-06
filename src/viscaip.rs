use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::task;
use tokio::select;
use tokio::sync::{mpsc, oneshot, broadcast};
use crate::uvierror::{UVIResult, UVIError};
use crate::MainEvent;
use crate::auto_uvc::CamCmd;

/* references:
- https://www.epiphan.com/userguides/LUMiO12x/Content/UserGuides/PTZ/3-operation/VISCAcommands.htm
- https://www.sony.net/Products/CameraSystem/CA/BRC_X1000_BRC_H800/Technical_Document/C456100121.pdf
- https://laiatech.com/wp-content/uploads/2021/07/NET-Visca-Commands.pdf
*/

fn int_to_nibbles(v : i64, size: usize) -> Vec<u8> {
    let mut p = v;
    let mut s: Vec<u8> = Vec::new();
    for _i in 0..size {
        s.insert(0, (p & 0xF) as u8);
        p = p >> 4;
    }
    return s;
}

fn sec_angle_to_nibbles(secang: i64, size: usize) -> Vec<u8> {
    return int_to_nibbles(secang*2359/36000, size);
}

fn nibbles_to_int(nibbles: &[u8]) -> i64 {
    let mut r: i64 = 0;
    for i in 0..nibbles.len() {
        r = (r<<4) + (nibbles[i] as i64);
    }
    if r & (0x8<<(4*nibbles.len()-4)) != 0 {
        r -= 0x1<<(4*nibbles.len());
    }
    return r;
}

fn nibbles_to_sec_angle(nibbles: &[u8]) -> i64 {
    return nibbles_to_int(nibbles)*36000/2359;
}

fn list_to_hex(l: &[u8]) -> String {
    let mut f = "".to_string();
    for i in 0..l.len() {
        f.push_str(&format!("{:02X} ", l[i]));
    }
    return f;
}

struct ViscaIpCon {
    ncam: u8,
    stream: TcpStream,
    main_chan: mpsc::Sender<MainEvent>,
    cam_chan: mpsc::UnboundedSender<CamCmd>,
    recvkill: broadcast::Receiver<()>
}

impl ViscaIpCon {
    async fn send_to_cam(&self, cmd: CamCmd) -> UVIResult<()> {
        self.cam_chan.send(cmd).map_err(|_x| UVIError::AsyncChannelClosed)
    }
    async fn send_datagram(&mut self, dg: &[u8]) {
        let mut buf = vec![0x91u8];
        buf.extend_from_slice(dg);
        buf.push(0xff);
        self.stream.write_all(&buf).await.unwrap();
    }
    async fn process(&mut self) -> UVIResult<()> {
        self.main_chan.send(MainEvent::NewViscaConnection(self.ncam, 
            self.stream.peer_addr()?)).await.map_err(|_x| UVIError::AsyncChannelClosed)?;
        let mut buf = Vec::new();
        let mut buf2 = vec![0u8;256];
        loop {
            tokio::select! {
                _ = self.recvkill.recv() => {
                    break;
                },
                read = self.stream.read(&mut buf2) => {
                    let n = match read {
                        Ok(n) if n == 0 => break,
                        Ok(n) => n,
                        Err(_e) => break
                    };
                    buf.extend_from_slice(&mut buf2[..n]);
                    let mut i = 0;
                    for p in 0..buf.len() {
                        if buf[p] == 0xFFu8 {
                            self.data_received(&buf[i..p]).await?;
                            i = p+1;
                        }
                    }
                    buf = buf[i..].to_vec();
                    if buf.len() > 200 {
                        break;
                    }        
                }
            }
        }
        self.main_chan.send(MainEvent::LostViscaConnection(self.ncam, 
            self.stream.peer_addr()?)).await.map_err(|_x| UVIError::AsyncChannelClosed)?;
        Ok(())
    }

    async fn data_received(&mut self, dg: &[u8]) -> UVIResult<()> {
        if dg.len() < 2 { return Ok(()); } // Ignore messages that are too short
        if dg[0] != 0x81 { return Ok(()); } // Ignore messages not addressed properly
        if dg[1] == 0x01 { // Command
            if dg[2] == 0x04 && dg[3] == 0x3f { // Cam Memory
                if dg[4] == 0x00 { // print('reset preset %d' % dg[5])
                    self.send_to_cam(CamCmd::ResetPreset(dg[5])).await?;
                } else if dg[4] == 0x01 { // print('save preset %d' % dg[5])
                    self.send_to_cam(CamCmd::RecordPreset(dg[5])).await?;
                } else if dg[4] == 0x02 { // print('recall preset %d' % dg[5])
                    self.send_to_cam(CamCmd::RecoverPreset(dg[5])).await?;
                }
            } else if dg[2] == 0x00 && dg[3] == 0x01 { // IF_Clear
                // println!("Clearing command buffer");
            } else if dg[2] == 0x06 && dg[3] == 0x04 { // Home
                self.send_to_cam(CamCmd::Home()).await?;
            } else if dg[2] == 0x06 && dg[3] == 0x01 { // move cam pan/tilt
                // seconds(degree/3600) per second
                let mut panspeed = ((dg[4] as i64) % (0x18+1)) * 3600; // 0x18 -> ~ 5sec for 180 degrees
                if dg[4] > 0x08 { panspeed = 2*panspeed; }
                if dg[4] > 0x12 { panspeed = 2*panspeed; }
                let tiltspeed = ((dg[5] as i64) % (0x14+1)) * 3600; // 0x14 -> ~2sec for 45 degrees
                let panmove:i64 = panspeed * (if dg[6] == 1 {-1} else if dg[6] == 2 {1} else {0});
                let tiltmove:i64 = tiltspeed * (if dg[7] == 1 {1} else if dg[7] == 2 {-1} else {0});
                self.send_to_cam(CamCmd::MoveContinuous(panmove, tiltmove)).await?;
            } else if dg[2] == 0x06 && dg[3] == 0x03 { // move pan/tilt relative
                let panmove = nibbles_to_sec_angle(&dg[6..10]);
                let tiltmove = nibbles_to_sec_angle(&dg[10..14]);
                self.send_to_cam(CamCmd::MoveRelative(panmove, tiltmove)).await?;
            } else if dg[2] == 0x06 && dg[3] == 0x02 { // move pan/tilt absolute
                let panpos = nibbles_to_sec_angle(&dg[6..10]);
                let tiltpos = nibbles_to_sec_angle(&dg[10..14]);
                self.send_to_cam(CamCmd::MoveAbsolute(panpos, tiltpos)).await?;
            } else if dg[2] == 0x04 && dg[3] == 0x07 { // move cam zoom
                let mut zoom:f64 = 0.0;
                if dg[4] == 2 { zoom = 1.0; }
                else if dg[4] == 3 { zoom = -1.0; }
                else if dg[4] & 0xF0 == 0x20 { zoom = ((1+(dg[4] & 0x7)) as f64)/8.0; }
                else if dg[4] & 0xF0 == 0x30 { zoom = -((1+(dg[4] & 0x7)) as f64)/8.0; }
                self.send_to_cam(CamCmd::ZoomContinuous(zoom)).await?;
            } else if dg[2] == 0x04 && dg[3] == 0x47 { // move cam zoom direct
                let zoom:f64 = (nibbles_to_int(&dg[4..8]) as f64)/(0x4000 as f64);
                self.send_to_cam(CamCmd::ZoomDirect(zoom)).await?;
            } else if dg[2] == 0x04 && dg[3] == 0x38 { // focus mode
                if dg[4] == 2 {
                    self.send_to_cam(CamCmd::AutoFocus(true)).await?;
                } else if dg[4] == 3 {
                    self.send_to_cam(CamCmd::AutoFocus(false)).await?;
                } else if dg[4] == 0x10 {
                    self.send_to_cam(CamCmd::AutoFocusToggle()).await?;
                }
            } else if dg[2] == 0x04 && dg[3] == 0x08 { // move cam focus
                let mut focus:f64 = 0.0;
                if dg[4] == 2 { focus = 1.0; }
                else if dg[4] == 3 { focus = -1.0; }
                else if dg[4] & 0xF0 == 0x20 { focus = ((1+(dg[4] & 0x7)) as f64)/8.0; }
                else if dg[4] & 0xF0 == 0x30 { focus = -((1+(dg[4] & 0x7)) as f64)/8.0; }
                self.send_to_cam(CamCmd::FocusContinuous(focus)).await?;
            } else if dg[2] == 0x04 && dg[3] == 0x48 { // move cam focus direct
                // pppp: F000 (Near) - 0000 (Far) -> 1.0 (Near) - 0.0 (Far)
                let focus:f64 = (nibbles_to_int(&dg[4..8]) as f64)/(0xF000 as f64);
                self.send_to_cam(CamCmd::FocusDirect(focus)).await?;
            } else if dg[2] == 0x04 && dg[3] == 0x18 { // one push focus
                if dg[4] == 1 {
                    self.send_to_cam(CamCmd::FocusOnePushTrigger()).await?;
                } else if dg[4] == 2 {
                    self.send_to_cam(CamCmd::FocusDirect(0.0)).await?; // FAR (infinite)
                }
            } else if dg[2] == 0x04 && dg[3] == 0x10 { // one push wb
                if dg[4] == 0x05 {
                    self.send_to_cam(CamCmd::WhiteBalanceTrigger()).await?;
                }
            } else if dg[2] == 0x04 && dg[3] == 0x35 { // set white balance
                self.send_to_cam(CamCmd::WhiteBalanceMode(dg[4])).await?;
            } else {
                println!("command unknown: {}", list_to_hex(&dg));
            }
            self.send_datagram(&[0x41u8]).await;
            self.send_datagram(&[0x51u8]).await;
        }
        else if dg[1] == 0x09 { // Inquiry
            if dg[2] == 0x00 && dg[3] == 0x02 { // CAM_VersionInq
                // print('CAM_VersionInq')
                self.send_datagram(&[0x50u8, 0x09,0x99, 0x00,0x01, 0x00,0x01, 0x02]).await;
            } else if dg[2] == 0x06 && dg[3] == 0x12 { // Pan-tiltPosInq
                let (s, r) = oneshot::channel();
                self.send_to_cam(CamCmd::QueryPanTilt(s)).await?;
                let pantilt = r.await.map_err(|_x| UVIError::AsyncChannelNoSender)?;
                let mut v = vec![0x50u8];
                v.extend(&sec_angle_to_nibbles(pantilt.0,5));
                v.extend(&sec_angle_to_nibbles(pantilt.1,4));
                self.send_datagram(&v).await;
            } else if dg[2] == 0x04 && dg[3] == 0x38 { // CAM_FocusModeInq
                let (s, r) = oneshot::channel();
                self.send_to_cam(CamCmd::QueryFocusMode(s)).await?;
                let mode = r.await.map_err(|_x| UVIError::AsyncChannelNoSender)?;
                let mut v = vec![0x50u8];
                v.extend([if mode {2u8} else {3u8}]);
                self.send_datagram(&v).await;
            } else if dg[2] == 0x04 && dg[3] == 0x35 { // CAM_WhiteBalInq
                let (s, r) = oneshot::channel();
                self.send_to_cam(CamCmd::QueryWhiteBalanceMode(s)).await?;
                let mode = r.await.map_err(|_x| UVIError::AsyncChannelNoSender)?;
                let mut v = vec![0x50u8];
                v.extend([mode]);
                self.send_datagram(&v).await;
            } else if dg[2] == 0x7e && dg[3] == 0x7e { // Block Inquiry
                //print('Block Inq '+str(dg[4]))
                if dg[4] == 0x00 { // Lens control
                    //y0 50 0u 0u 0u 0u 00 00 0v 0v 0v 0v 00 0w 00 FF
                    //uuuu: Zoom Position
                    //vvvv: Focus Position
                    //w.bit0: Focus Mode 1: Auto 0: Manual
                    let (s, r) = oneshot::channel();
                    self.send_to_cam(CamCmd::QueryFocusMode(s)).await?;
                    let mode = r.await.map_err(|_x| UVIError::AsyncChannelNoSender)?;
                    let mut v = vec![0x50u8];
                    for _ in 0..11 { v.extend([0u8]); }
                    v.extend([if mode {1u8} else {0u8}]);
                    v.extend([0u8]);
                    self.send_datagram(&v).await;
                } else if dg[4] == 0x01 { // Camera control
                    //y0 50 0p 0p 0q 0q 0r 0s tt 0u vv ww 00 xx 0z FF
                    //pp: R_Gain
                    //qq: B_Gain
                    //r: WB Mode
                    //s: Aperture
                    //tt: AE Mode
                    //u.bit2: Back Light
                    //u.bit1: Exposure Comp.
                    //vv: Shutter Position
                    //ww: Iris Position
                    //xx: Bright Position
                    //z: Exposure Comp. Position
                    self.send_datagram(&[0x60u8, 0x02u8]).await;
                    //self.send_datagram([0x50] + [0]*13)
                } else if dg[4] == 0x03 { // Other enlargement 1
                    //y0 50 00 00 00 00 00 00 00 0p 0q rr 0s 0t 0u FF
                    //p: AF sensitivity
                    //q.bit0: Picture flip(1:On, 0:Off)
                    // rr.bit6~3: Color Gain(0h(60%) to Eh(200%))
                    //s: Flip(0: Off, 1:Flip-H, 2:Flip-V, 3:Flip-HV)
                    //t.bit2~0: NR2D Level
                    //u: Gain Limit
                    self.send_datagram(&[0x60u8, 0x02u8]).await;
                } else {
                    self.send_datagram(&[0x60u8, 0x02u8]).await;
                }
            } else {
                println!("Inquiry unknown: {}", list_to_hex(&dg));
            }
            self.send_datagram(&[0x60u8, 0x02u8]).await;
        }
        else {
            println!("Not recognized: {}", list_to_hex(&dg));
            self.send_datagram(&[0x60u8, 0x02u8]).await;
        }
        Ok(())
    }
}

pub async fn activate_visca_port(port: u32, ncam: u8, main_chan: mpsc::Sender<MainEvent>, 
        cam_chan: mpsc::UnboundedSender<CamCmd>, ncamdead: mpsc::Sender<u8>) -> UVIResult<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}",port)).await?;
    //println!("Listening on {}", listener.local_addr()?);
    task::spawn(async move {
        let (sendkill, mut recvkill) = broadcast::channel(1);
        loop {
            select! {
                _ = recvkill.recv() => {
                    break;
                },
                acc = listener.accept() => {
                    let (socket, _socket_addr) = acc.expect("Bad accept?");
                    let mut v = ViscaIpCon {
                        ncam: ncam,
                        stream: socket,
                        main_chan: main_chan.clone(),
                        cam_chan: cam_chan.clone(),
                        recvkill: sendkill.subscribe()
                    };
                    let sendkill = sendkill.clone();
                    task::spawn(async move {
                        match v.process().await {
                            Err(e) => {
                                eprintln!("Closing ViscaIP connection for error: {}", e);
                                sendkill.send(()).ok();
                            }
                            _ => ()
                        }
                    });
                }
            }
        }
        ncamdead.send(ncam).await.ok();
    });
    Ok(())
}

