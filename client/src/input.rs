use std::mem::{self, MaybeUninit};
use std::ptr;
use std::slice;
use std::thread;
use std::io::Read;
use std::fs::File;
use std::sync::mpsc::{self, Sender, Receiver};
use std::os::unix::io::AsRawFd;
use std::ffi::CString;



use fxhash::FxHashMap;
use crate::framebuffer::Display;
use crate::settings::ButtonScheme;
use crate::device::CURRENT_DEVICE;
use crate::geom::{Point, LinearDir};
use anyhow::{Error, Context};


// Event types
pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_ABS: u16 = 0x03;
pub const EV_MSC: u16 = 0x04;

// Event codes
pub const ABS_MT_SLOT: u16 = 0x2f;
pub const ABS_MT_TRACKING_ID: u16 = 0x39;
pub const ABS_MT_POSITION_X: u16 = 0x35;
pub const ABS_MT_POSITION_Y: u16 = 0x36;
pub const ABS_MT_PRESSURE: u16 = 0x3a;
pub const ABS_MT_TOUCH_MAJOR: u16 = 0x30;
pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;
pub const ABS_PRESSURE: u16 = 0x18;
pub const MSC_RAW: u16 = 0x03;
pub const SYN_REPORT: u16 = 0x00;

// Event values
pub const MSC_RAW_GSENSOR_PORTRAIT_DOWN: i32 = 0x17;
pub const MSC_RAW_GSENSOR_PORTRAIT_UP: i32 = 0x18;
pub const MSC_RAW_GSENSOR_LANDSCAPE_RIGHT: i32 = 0x19;
pub const MSC_RAW_GSENSOR_LANDSCAPE_LEFT: i32 = 0x1a;
// pub const MSC_RAW_GSENSOR_BACK: i32 = 0x1b;
// pub const MSC_RAW_GSENSOR_FRONT: i32 = 0x1c;

// The indices of this clockwise ordering of the sensor values match the Forma's rotation values.
pub const GYROSCOPE_ROTATIONS: [i32; 4] = [MSC_RAW_GSENSOR_LANDSCAPE_LEFT, MSC_RAW_GSENSOR_PORTRAIT_UP,
    MSC_RAW_GSENSOR_LANDSCAPE_RIGHT, MSC_RAW_GSENSOR_PORTRAIT_DOWN];

pub const VAL_RELEASE: i32 = 0;
pub const VAL_PRESS: i32 = 1;
pub const VAL_REPEAT: i32 = 2;

// Key codes
pub const KEY_POWER: u16 = 116;
pub const KEY_HOME: u16 = 102;
pub const KEY_LIGHT: u16 = 90;
pub const KEY_BACKWARD: u16 = 193;
pub const KEY_FORWARD: u16 = 194;
pub const PEN_ERASE: u16 = 331;
pub const PEN_HIGHLIGHT: u16 = 332;
pub const SLEEP_COVER: [u16; 2] = [59, 35];
// Synthetic touch button
pub const BTN_TOUCH: u16 = 330;
// The following key codes are fake, and are used to support
// software toggles within this design
pub const KEY_ROTATE_DISPLAY: u16 = 0xffff;
pub const KEY_BUTTON_SCHEME: u16 = 0xfffe;

pub const SINGLE_TOUCH_CODES: TouchCodes = TouchCodes {
    pressure: ABS_PRESSURE,
    x: ABS_X,
    y: ABS_Y,
};

pub const MULTI_TOUCH_CODES_A: TouchCodes = TouchCodes {
    pressure: ABS_MT_TOUCH_MAJOR,
    x: ABS_MT_POSITION_X,
    y: ABS_MT_POSITION_Y,
};

pub const MULTI_TOUCH_CODES_B: TouchCodes = TouchCodes {
    pressure: ABS_MT_PRESSURE,
    .. MULTI_TOUCH_CODES_A
};

#[repr(C)]
pub struct InputEvent {
    pub time: libc::timeval,
    pub kind: u16, // type
    pub code: u16,
    pub value: i32,
}

// Handle different touch protocols
#[derive(Debug)]
pub struct TouchCodes {
    pressure: u16,
    x: u16,
    y: u16,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TouchProto {
    Single,
    MultiA,
    MultiB, // Pressure won't indicate a finger release.
    MultiC,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum FingerStatus {
    Down,
    Motion,
    Up,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ButtonStatus {
    Pressed,
    Released,
    Repeated,
}

impl ButtonStatus {
    pub fn try_from_raw(value: i32) -> Option<ButtonStatus> {
        match value {
            VAL_RELEASE => Some(ButtonStatus::Released),
            VAL_PRESS => Some(ButtonStatus::Pressed),
            VAL_REPEAT => Some(ButtonStatus::Repeated),
            _ => None,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ButtonCode {
    Power,
    Home,
    Light,
    Backward,
    Forward,
    Erase,
    Highlight,
    Raw(u16),
}

impl ButtonCode {
    fn from_raw(code: u16, rotation: i8, button_scheme: ButtonScheme) -> ButtonCode {
        match code {
            KEY_POWER => ButtonCode::Power,
            KEY_HOME => ButtonCode::Home,
            KEY_LIGHT => ButtonCode::Light,
            KEY_BACKWARD => resolve_button_direction(LinearDir::Backward, rotation, button_scheme),
            KEY_FORWARD => resolve_button_direction(LinearDir::Forward, rotation, button_scheme),
            PEN_ERASE => ButtonCode::Erase,
            PEN_HIGHLIGHT => ButtonCode::Highlight,
            _ => ButtonCode::Raw(code)
        }
    }
}

fn resolve_button_direction(mut direction: LinearDir, rotation: i8, button_scheme: ButtonScheme) -> ButtonCode {
    if (CURRENT_DEVICE.should_invert_buttons(rotation)) ^ (button_scheme == ButtonScheme::Inverted) {
        direction = direction.opposite();
    }

    if direction == LinearDir::Forward {
        return ButtonCode::Forward;
    }

    ButtonCode::Backward
}

pub fn display_rotate_event(n: i8) -> InputEvent {
    let mut tp = libc::timeval { tv_sec: 0, tv_usec: 0 };
    unsafe { libc::gettimeofday(&mut tp, ptr::null_mut()); }
    InputEvent {
        time: tp,
        kind: EV_KEY,
        code: KEY_ROTATE_DISPLAY,
        value: n as i32,
    }
}

pub fn button_scheme_event(v: i32) -> InputEvent {
    let mut tp = libc::timeval { tv_sec: 0, tv_usec: 0 };
    unsafe { libc::gettimeofday(&mut tp, ptr::null_mut()); }
    InputEvent {
        time: tp,
        kind: EV_KEY,
        code: KEY_BUTTON_SCHEME,
        value: v,
    }
}

#[derive(Debug, Copy, Clone)]
pub enum DeviceEvent {
    Finger {
        id: i32,
        time: f64,
        status: FingerStatus,
        position: Point,
    },
    Button {
        time: f64,
        code: ButtonCode,
        status: ButtonStatus,
    },
    Plug(PowerSource),
    Unplug(PowerSource),
    RotateScreen(i8),
    CoverOn,
    CoverOff,
    NetUp,
    UserActivity,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PowerSource {
    Host,
    Wall,
}

pub fn seconds(time: libc::timeval) -> f64 {
    time.tv_sec as f64 + time.tv_usec as f64 / 1e6
}

pub fn raw_events(paths: Vec<String>) -> (Sender<InputEvent>, Receiver<InputEvent>) {
    let (tx, rx) = mpsc::channel();
    let tx2 = tx.clone();
    thread::spawn(move || parse_raw_events(&paths, &tx));
    (tx2, rx)
}

pub fn
parse_raw_events(paths: &[String], tx: &Sender<InputEvent>) -> Result<(), Error> {
    let mut files = Vec::new();
    let mut pfds = Vec::new();

    for path in paths.iter() {
        let file = File::open(path)
            .with_context(|| format!("can't open input file {}", path))?;
        let fd = file.as_raw_fd();
        files.push(file);
        pfds.push(libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        });
    }

    loop {
        let ret = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, -1) };
        if ret < 0 {
            break;
        }
        for (pfd, mut file) in pfds.iter().zip(&files) {
            if pfd.revents & libc::POLLIN != 0 {
                let mut input_event = MaybeUninit::<InputEvent>::uninit();
                unsafe {
                    let event_slice = slice::from_raw_parts_mut(input_event.as_mut_ptr() as *mut u8,
                                                                mem::size_of::<InputEvent>());
                    if file.read_exact(event_slice).is_err() {
                        break;
                    }
                    tx.send(input_event.assume_init()).ok();
                }
            }
        }
    }

    Ok(())
}

pub fn usb_events() -> Receiver<DeviceEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || parse_usb_events(&tx));
    rx
}

fn parse_usb_events(tx: &Sender<DeviceEvent>) {
    let path = CString::new("/tmp/nickel-hardware-status").unwrap();
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_NONBLOCK | libc::O_RDWR) };

    if fd < 0 {
        return;
    }

    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };

    const BUF_LEN: usize = 256;

    loop {
        let ret = unsafe { libc::poll(&mut pfd as *mut libc::pollfd, 1, -1) };

        if ret < 0 {
            break;
        }

        let buf = CString::new(vec![1; BUF_LEN]).unwrap();
        let c_buf = buf.into_raw();

        if pfd.revents & libc::POLLIN != 0 {
            let n = unsafe { libc::read(fd, c_buf as *mut libc::c_void, BUF_LEN as libc::size_t) };
            let buf = unsafe { CString::from_raw(c_buf) };
            if n > 0 {
                if let Ok(s) = buf.to_str() {
                    for msg in s[..n as usize].lines() {
                        if msg == "usb plug add" {
                            tx.send(DeviceEvent::Plug(PowerSource::Host)).ok();
                        } else if msg == "usb plug remove" {
                            tx.send(DeviceEvent::Unplug(PowerSource::Host)).ok();
                        } else if msg == "usb ac add" {
                            tx.send(DeviceEvent::Plug(PowerSource::Wall)).ok();
                        } else if msg == "usb ac remove" {
                            tx.send(DeviceEvent::Unplug(PowerSource::Wall)).ok();
                        } else if msg.starts_with("network bound") {
                            tx.send(DeviceEvent::NetUp).ok();
                        }
                    }
                }
            } else {
                break;
            }
        }
    }
}

pub fn device_events(rx: Receiver<InputEvent>, display: Display, button_scheme: ButtonScheme) -> Receiver<DeviceEvent> {
    let (ty, ry) = mpsc::channel();
    thread::spawn(move || parse_device_events(&rx, &ty, display, button_scheme));
    ry
}

struct TouchState {
    position: Point,
    pressure: i32,
}

impl Default for TouchState {
    fn default() -> Self {
        TouchState {
            position: Point::default(),
            pressure: 0,
        }
    }
}

/// Return the contact being updated.
///
/// Protocol B move frames omit `ABS_MT_TRACKING_ID`. When `slot_protocol` is set,
/// revive the contact that is already down so those frames still move it. A
/// tracking id of -1 puts the contact in `lifted` and must not revive it.
fn active_contact<'a>(
    slot_protocol: bool,
    id: i32,
    lifted: &[i32],
    fingers: &FxHashMap<i32, Point>,
    packets: &'a mut FxHashMap<i32, TouchState>,
) -> Option<&'a mut TouchState> {
    if id < 0 || lifted.contains(&id) {
        return None;
    }
    if !packets.contains_key(&id) {
        let position = match fingers.get(&id) {
            Some(&position) if slot_protocol => position,
            _ => return None,
        };
        packets.insert(id, TouchState { position, pressure: 1 });
    }
    packets.get_mut(&id)
}

pub fn parse_device_events(rx: &Receiver<InputEvent>, ty: &Sender<DeviceEvent>, display: Display, button_scheme: ButtonScheme) {
    let mut id = 0;
    let mut slot: i32 = 0;
    let mut last_activity = -60;
    let Display { mut dims, mut rotation } = display;


    let mut fingers: FxHashMap<i32, Point> = FxHashMap::default();
    let mut packets: FxHashMap<i32, TouchState> = FxHashMap::default();
    let mut slot_ids: FxHashMap<i32, i32> = FxHashMap::default();
    let mut lifted: Vec<i32> = Vec::new();
    let proto = CURRENT_DEVICE.proto;
    let slot_protocol = CURRENT_DEVICE.mt_protocol_b();

    let mut tc = match proto {
        TouchProto::Single => SINGLE_TOUCH_CODES,
        TouchProto::MultiA => MULTI_TOUCH_CODES_A,
        TouchProto::MultiB => MULTI_TOUCH_CODES_B,
        TouchProto::MultiC => MULTI_TOUCH_CODES_B,
    };

    if proto == TouchProto::Single {
        packets.insert(id, TouchState::default());
    }

    let (mut mirror_x, mut mirror_y) = CURRENT_DEVICE.should_mirror_axes(rotation);
    if CURRENT_DEVICE.should_swap_axes(rotation) {
        mem::swap(&mut tc.x, &mut tc.y);


    }

    let mut button_scheme = button_scheme;

    while let Ok(evt) = rx.recv() {
        if evt.kind == EV_ABS {
            if evt.code == ABS_MT_SLOT {
                slot = evt.value;
                if let Some(&mapped) = slot_ids.get(&slot) {
                    id = mapped;
                }
            } else if evt.code == ABS_MT_TRACKING_ID {
                if evt.value >= 0 {
                    id = evt.value;
                    if slot_protocol {
                        slot_ids.insert(slot, id);
                        lifted.retain(|&lifted_id| lifted_id != id);
                        let position = fingers.get(&id).copied().unwrap_or_default();
                        // A live tracking id is a contact even when this frame has no pressure sample.
                        packets.entry(id).or_insert(TouchState { position, pressure: 1 });
                    } else {
                        packets.insert(id, TouchState::default());
                    }
                } else if slot_protocol && id >= 0 {
                    if !lifted.contains(&id) {
                        lifted.push(id);
                    }
                    packets.remove(&id);
                    slot_ids.remove(&slot);
                    id = -1;
                }
            } else if evt.code == tc.x {
                if let Some(state) = active_contact(slot_protocol, id, &lifted, &fingers, &mut packets) {
                    state.position.x = if mirror_x {
                        dims.0 as i32 - 1 - evt.value
                    } else {
                        evt.value
                    };
                }
            } else if evt.code == tc.y {
                if let Some(state) = active_contact(slot_protocol, id, &lifted, &fingers, &mut packets) {
                    state.position.y = if mirror_y {
                        dims.1 as i32 - 1 - evt.value
                    } else {
                        evt.value
                    };
                }
            } else if evt.code == tc.pressure {
                if let Some(state) = active_contact(slot_protocol, id, &lifted, &fingers, &mut packets) {
                    state.pressure = evt.value;
                    if proto == TouchProto::Single && CURRENT_DEVICE.mark() == 3 && state.pressure == 0 {
                        state.position.x = dims.0 as i32 - 1 - state.position.x;
                        mem::swap(&mut state.position.x, &mut state.position.y);
                    }
                }
            }
        } else if evt.kind == EV_SYN && evt.code == SYN_REPORT {
            // The absolute value accounts for the wrapping around that might occur,
            // since `tv_sec` can't grow forever.
            if (evt.time.tv_sec - last_activity).abs() >= 60 {
                last_activity = evt.time.tv_sec;
                ty.send(DeviceEvent::UserActivity).ok();
            }

            // Snow-style MultiB reports every active contact in each frame, so a
            // missing packet is a lift. Protocol B only reports what changed, and
            // lifts with tracking id -1 (handled below).
            if proto == TouchProto::MultiB && !slot_protocol {
                fingers.retain(|other_id, other_position| {
                    packets.contains_key(&other_id) ||
                        ty.send(DeviceEvent::Finger {
                            id: *other_id,
                            time: seconds(evt.time),
                            status: FingerStatus::Up,
                            position: *other_position,
                        }).is_err()
                });
            }

            for (&id, state) in &packets {
                if let Some(&pos) = fingers.get(&id) {
                    if state.pressure > 0 {
                        if state.position != pos {
                            ty.send(DeviceEvent::Finger {
                                id,
                                time: seconds(evt.time),
                                status: FingerStatus::Motion,
                                position: state.position,
                            }).unwrap();
                            fingers.insert(id, state.position);
                        }
                    } else {
                        ty.send(DeviceEvent::Finger {
                            id,
                            time: seconds(evt.time),
                            status: FingerStatus::Up,
                            position: state.position,
                        }).unwrap();
                        fingers.remove(&id);
                    }
                } else if state.pressure > 0 {
                    ty.send(DeviceEvent::Finger {
                        id,
                        time: seconds(evt.time),
                        status: FingerStatus::Down,
                        position: state.position,
                    }).unwrap();
                    fingers.insert(id, state.position);
                }
            }

            if slot_protocol {
                for lifted_id in lifted.drain(..) {
                    if let Some(position) = fingers.remove(&lifted_id) {
                        ty.send(DeviceEvent::Finger {
                            id: lifted_id,
                            time: seconds(evt.time),
                            status: FingerStatus::Up,
                            position,
                        }).ok();
                    }
                }
            }

            if proto != TouchProto::Single {
                packets.clear();
            }
        } else if evt.kind == EV_KEY {
            if SLEEP_COVER.contains(&evt.code) {
                if evt.value == VAL_PRESS {

                    ty.send(DeviceEvent::CoverOn).ok();
                } else if evt.value == VAL_RELEASE {

                    ty.send(DeviceEvent::CoverOff).ok();
                } else if evt.value == VAL_REPEAT {

                    ty.send(DeviceEvent::CoverOn).ok();
                }
            } else if evt.code == KEY_BUTTON_SCHEME {
                if evt.value == VAL_PRESS {

                    button_scheme = ButtonScheme::Inverted;
                } else {

                    button_scheme = ButtonScheme::Natural;
                }
            } else if evt.code == KEY_ROTATE_DISPLAY {

                let next_rotation = evt.value as i8;
                if next_rotation != rotation {
                    let delta = (rotation - next_rotation).abs();
                    if delta % 2 == 1 {
                        mem::swap(&mut tc.x, &mut tc.y);
                        mem::swap(&mut dims.0, &mut dims.1);
                    }
                    rotation = next_rotation;
                    let should_mirror = CURRENT_DEVICE.should_mirror_axes(rotation);
                    mirror_x = should_mirror.0;
                    mirror_y = should_mirror.1;
                }
            } else if evt.code != BTN_TOUCH {

                if let Some(button_status) = ButtonStatus::try_from_raw(evt.value) {
                    ty.send(DeviceEvent::Button {
                        time: seconds(evt.time),
                        code: ButtonCode::from_raw(evt.code, rotation, button_scheme),
                        status: button_status,
                    }).unwrap();
                }
            }
        } else if evt.kind == EV_MSC && evt.code == MSC_RAW {

            if evt.value >= MSC_RAW_GSENSOR_PORTRAIT_DOWN && evt.value <= MSC_RAW_GSENSOR_LANDSCAPE_LEFT {
                let next_rotation = GYROSCOPE_ROTATIONS.iter().position(|&v| v == evt.value)
                    .map(|i| CURRENT_DEVICE.transformed_gyroscope_rotation(i as i8));
                if let Some(next_rotation) = next_rotation {
                    ty.send(DeviceEvent::RotateScreen(next_rotation)).ok();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use crate::device::{Model, CURRENT_DEVICE};
    use crate::framebuffer::Display;
    use crate::settings::ButtonScheme;

    use super::{
        device_events, DeviceEvent, FingerStatus, InputEvent, ABS_MT_POSITION_X, ABS_MT_POSITION_Y,
        ABS_MT_PRESSURE, ABS_MT_SLOT, ABS_MT_TRACKING_ID, EV_ABS, EV_SYN, SYN_REPORT,
    };

    fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
        InputEvent {
            time: libc::timeval { tv_sec: 1, tv_usec: 0 },
            kind,
            code,
            value,
        }
    }

    fn abs(code: u16, value: i32) -> InputEvent {
        ev(EV_ABS, code, value)
    }

    fn syn() -> InputEvent {
        ev(EV_SYN, SYN_REPORT, 0)
    }

    fn finger_events(events: Vec<InputEvent>) -> Vec<(FingerStatus, i32, i32, i32)> {
        let (tx, rx) = mpsc::channel();
        let out = device_events(
            rx,
            Display { dims: (1264, 1680), rotation: 0 },
            ButtonScheme::Natural,
        );
        for event in events {
            tx.send(event).unwrap();
        }
        drop(tx);
        out.into_iter()
            .filter_map(|event| match event {
                DeviceEvent::Finger { status, id, position, .. } => {
                    Some((status, id, position.x, position.y))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn libra_colour_protocol_b_tracks_kernel_ids_across_delta_frames() {
        // CURRENT_DEVICE is process-global. Set the codename before the first touch.
        std::env::set_var("PRODUCT", "monza");
        assert_eq!(CURRENT_DEVICE.model, Model::LibraColour);
        assert!(CURRENT_DEVICE.mt_protocol_b());

        let events = finger_events(vec![
            // Contact 45, the sort of id the kernel assigns after a few taps.
            abs(ABS_MT_SLOT, 0),
            abs(ABS_MT_TRACKING_ID, 45),
            abs(ABS_MT_POSITION_X, 10),
            abs(ABS_MT_POSITION_Y, 20),
            abs(ABS_MT_PRESSURE, 40),
            syn(),
            // An empty report must not look like a lift.
            syn(),
            // Move frame: slot and coordinates only. No tracking id.
            abs(ABS_MT_SLOT, 0),
            abs(ABS_MT_POSITION_X, 15),
            abs(ABS_MT_POSITION_Y, 25),
            syn(),
            // Second finger lands while the first is still down.
            abs(ABS_MT_SLOT, 1),
            abs(ABS_MT_TRACKING_ID, 46),
            abs(ABS_MT_POSITION_X, 80),
            abs(ABS_MT_POSITION_Y, 90),
            syn(),
            // Moving finger 45 must not release finger 46.
            abs(ABS_MT_SLOT, 0),
            abs(ABS_MT_POSITION_X, 16),
            abs(ABS_MT_POSITION_Y, 26),
            syn(),
            abs(ABS_MT_SLOT, 0),
            abs(ABS_MT_TRACKING_ID, -1),
            syn(),
            // Id 3 is outside the old 0/1/2 whitelist, and this frame has no pressure.
            abs(ABS_MT_SLOT, 0),
            abs(ABS_MT_TRACKING_ID, 3),
            abs(ABS_MT_POSITION_X, 1),
            abs(ABS_MT_POSITION_Y, 2),
            syn(),
            abs(ABS_MT_SLOT, 1),
            abs(ABS_MT_TRACKING_ID, -1),
            syn(),
            abs(ABS_MT_SLOT, 0),
            abs(ABS_MT_TRACKING_ID, -1),
            syn(),
        ]);

        assert_eq!(
            events,
            vec![
                (FingerStatus::Down, 45, 10, 20),
                (FingerStatus::Motion, 45, 15, 25),
                (FingerStatus::Down, 46, 80, 90),
                (FingerStatus::Motion, 45, 16, 26),
                (FingerStatus::Up, 45, 16, 26),
                (FingerStatus::Down, 3, 1, 2),
                (FingerStatus::Up, 46, 80, 90),
                (FingerStatus::Up, 3, 1, 2),
            ]
        );
    }
}

