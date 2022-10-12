# webcam-visca-ip

## Visca IP protocol for USB PTZ WebCam in Windows/Linux
Status of the software: beta. Should work on most situations.

## What is this?

It's a way to allow the OBS (Open Broadcaster Software) to control USB PTZ Cameras.

## How to use:
- Download the precompiled software from the release tab;
- Install OBS PTZ plugin on OBS;
- Connect USB camera(s);
- Run this softare (Windows or Linux version) and start to use. There is no configuration to do, the localhost TCP ports 5678, 5679... are activated for each camera found. The application window gives some hints;
- Configure OBS PTZ:
  - Open PTZ panel and dock it close to the Sources Panel;
  - Configure the PTZs using the host "localhost" and ports 5678 for camera 1, 5679 for camera 2, etc.

## Observations
- Presets are saved on the user configuration directory and are associated to the sequence of the cameras detected by the computer.

