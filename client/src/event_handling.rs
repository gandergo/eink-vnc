use std::sync::mpsc::{self, Sender, Receiver};
use std::time::{Duration, Instant};
use crate::device::CURRENT_DEVICE;
use crate::framebuffer::Framebuffer;
use crate::geom::Dir;
use crate::gesture::{Event, GestureEvent};
use crate::input::{ButtonCode, ButtonStatus, DeviceEvent, FingerStatus};
use crate::vnc::{Client, Rect};

pub struct event_params {
    pub has_drawn_once: bool,
    pub finger_down_count: Instant,
    pub exit_to_nickel:bool,
    pub exit_to_gui:bool,
    pub x_offset:u32,
    pub y_offset:u32,
    /// A drag ended in this batch. The panel should show the latest frame now.
    pub drag_ended: bool,
    /// Keep skipping e-ink frames until this instant, then paint the settled page.
    pub page_scroll_until: Option<Instant>,
}

impl event_params {
    pub fn handle_events(rx: &Receiver<Event>, scale_factor: f32, width: u16, height: u16,
                         fb_width: u32, fb_height: u32, x_padding: u32, y_padding: u32, mut x_offset: u32, mut y_offset: u32,
                         vnc: &mut Option<Client>, mut finger_down_count: Instant, finger_seconds: Duration, fb: &mut Box<dyn Framebuffer>,
                         panning: bool, mut has_drawn_once: bool, scale: bool, long_tap: bool, gui_enabled:bool, disable_touch:bool,
                         disable_exit_via_hold:bool, exit_duration:u32,
                         finger_down: &mut bool, dragging: &mut bool,
                         ) -> event_params {
        // Drain every touch sample waiting in the queue. One sample per
        // framebuffer refresh is what makes a drag crawl frame by frame.
        let mut drag_ended = false;
        let mut page_scroll_until = None;
        // One page per pass. Kernel auto-repeat during the gesture is dropped
        // so a held button does not queue a stack of extra pages.
        let mut paged_this_batch = false;
        while let Ok(evt) = rx.try_recv() {
            match evt {
                Event::Device(de) => {
                    match de {
                        DeviceEvent::Finger {
                            status,
                            position,
                            ..
                        } => {
                            // Libra Colour reports kernel tracking ids (0, 1, 2, 3, …),
                            // not a recycled slot index. Any non-negative id is a real contact.
                                    match status {
                                        FingerStatus::Up => {
                                            if *dragging {
                                                drag_ended = true;
                                            }
                                            *finger_down = false;
                                            *dragging = false;
                                            //we only want send right click once we release long_tap
                                            if scale && !disable_touch {
                                                if long_tap {
                                                    if finger_down_count.elapsed() > finger_seconds
                                                    {
                                                         vnc.as_mut().unwrap().send_pointer_event(
                                                            0x04,
                                                            (((position.x as f32
                                                                - x_padding as f32)
                                                                / scale_factor)
                                                                as u16)
                                                                .clamp(0, width as u16),
                                                            (((position.y as f32
                                                                - y_padding as f32)
                                                                / scale_factor)
                                                                as u16)
                                                                .clamp(0, height as u16),
                                                        )
                                                            .unwrap();
                                                         vnc.as_mut().unwrap().send_pointer_event(
                                                            0x00,
                                                            (((position.x as f32
                                                                - x_padding as f32)
                                                                / scale_factor)
                                                                as u16)
                                                                .clamp(0, width as u16),
                                                            (((position.y as f32
                                                                - y_padding as f32)
                                                                / scale_factor)
                                                                as u16)
                                                                .clamp(0, height as u16),
                                                        )
                                                            .unwrap();
                                                        //dbg!(((position.x as f32-x_padding as f32) / scale_factor) as u16, ((position.y as f32-y_padding as f32) / scale_factor) as u16);
                                                    }
                                                } else {
                                                     vnc.as_mut().unwrap().send_pointer_event(
                                                        0x00,
                                                        (((position.x as f32 - x_padding as f32)
                                                            / scale_factor)
                                                            as u16)
                                                            .clamp(0, width as u16),
                                                        (((position.y as f32 - y_padding as f32)
                                                            / scale_factor)
                                                            as u16)
                                                            .clamp(0, height as u16),
                                                    )
                                                        .unwrap();
                                                    //dbg!(((position.x as f32 - x_padding as f32) / scale_factor) as u16, ((position.y as f32 - y_padding as f32) / scale_factor) as u16);
                                                }
                                            } else if !disable_touch {
                                                if long_tap {
                                                    if finger_down_count.elapsed() > finger_seconds {
                                                         vnc.as_mut().unwrap().send_pointer_event(
                                                            0x04,
                                                            ((position.x as i16 - x_padding as i16
                                                                + x_offset as i16)
                                                                as u16)
                                                                .clamp(0, width as u16),
                                                            ((position.y as i16 - y_padding as i16
                                                                + y_offset as i16)
                                                                as u16)
                                                                .clamp(0, height as u16),
                                                        )
                                                            .unwrap();
                                                         vnc.as_mut().unwrap().send_pointer_event(
                                                            0x00,
                                                            ((position.x as i16 - x_padding as i16
                                                                + x_offset as i16)
                                                                as u16)
                                                                .clamp(0, width as u16),
                                                            ((position.y as i16 - y_padding as i16
                                                                + y_offset as i16)
                                                                as u16)
                                                                .clamp(0, height as u16),
                                                        )
                                                            .unwrap();
                                                        //dbg!(position.x as u16-x_padding as u16, position.y as u16-y_padding as u16);

                                                    }
                                                } else {
                                                    if panning {
                                                         vnc.as_mut().unwrap().send_pointer_event(
                                                            0x01,
                                                            ((position.x as i16 - x_padding as i16
                                                                + x_offset as i16)
                                                                as u16)
                                                                .clamp(0, width as u16),
                                                            ((position.y as i16 - y_padding as i16
                                                                + y_offset as i16)
                                                                as u16)
                                                                .clamp(0, height as u16),
                                                        )
                                                            .unwrap();
                                                         vnc.as_mut().unwrap().send_pointer_event(
                                                            0x00,
                                                            ((position.x as i16 - x_padding as i16
                                                                + x_offset as i16)
                                                                as u16)
                                                                .clamp(0, width as u16),
                                                            ((position.y as i16 - y_padding as i16
                                                                + y_offset as i16)
                                                                as u16)
                                                                .clamp(0, height as u16),
                                                        )
                                                            .unwrap();
                                                        //dbg!(position.x as u16-x_padding as u16, position.y as u16-y_padding as u16);
                                                    } else {
                                                         vnc.as_mut().unwrap().send_pointer_event(
                                                            0x00,
                                                            ((position.x as i16 - x_padding as i16
                                                                + x_offset as i16)
                                                                as u16)
                                                                .clamp(0, width as u16),
                                                            ((position.y as i16 - y_padding as i16
                                                                + y_offset as i16)
                                                                as u16)
                                                                .clamp(0, height as u16),
                                                        )
                                                            .unwrap();
                                                        //dbg!(position.x as u16-x_padding as u16, position.y as u16-y_padding as u16);
                                                    }
                                                }
                                            };
                                            if finger_down_count.elapsed() > Duration::from_secs(exit_duration as u64) && !disable_exit_via_hold {
                                                if gui_enabled {
                                                    return event_params {
                                                        has_drawn_once,
                                                        finger_down_count,
                                                        exit_to_nickel: if gui_enabled {false} else {true},
                                                        exit_to_gui: if gui_enabled {true} else {false},
                                                        x_offset,
                                                        y_offset,
                                                        drag_ended,
                                                        page_scroll_until,
                                                    }
                                                } else {
                                                    return event_params {
                                                        has_drawn_once,
                                                        finger_down_count,
                                                        exit_to_nickel: if gui_enabled {false} else {true},
                                                        exit_to_gui: if gui_enabled {true} else {false},
                                                        x_offset,
                                                        y_offset,
                                                        drag_ended,
                                                        page_scroll_until,
                                                    }
                                                }
                                            };
                                        }
                                        FingerStatus::Down => {
                                            if !disable_touch && !panning {
                                                *finger_down = true;
                                                *dragging = false;
                                            }
                                            if scale && !disable_touch {
                                                if panning {
                                                    finger_down_count = Instant::now();
                                                } else {
                                                     vnc.as_mut().unwrap().send_pointer_event(
                                                        0x01,
                                                        (((position.x as f32 - x_padding as f32)
                                                            / scale_factor)
                                                            as u16)
                                                            .clamp(0, width as u16),
                                                        (((position.y as f32 - y_padding as f32)
                                                            / scale_factor)
                                                            as u16)
                                                            .clamp(0, height as u16),
                                                    )
                                                        .unwrap();
                                                    finger_down_count = Instant::now();
                                                    //dbg!((((position.x as f32 - x_padding as f32)/ scale_factor) as u16).clamp(0,width as u16),
                                                    // (((position.y as f32 - y_padding as f32)/ scale_factor) as u16).clamp(0,height as u16));
                                                }

                                            } else if !disable_touch {
                                                if panning {
                                                    finger_down_count = Instant::now();
                                                } else {
                                                     vnc.as_mut().unwrap().send_pointer_event(
                                                        0x01,
                                                        ((position.x as i16 - x_padding as i16
                                                            + x_offset as i16)
                                                            as u16)
                                                            .clamp(0, width as u16),
                                                        ((position.y as i16 - y_padding as i16
                                                            + y_offset as i16)
                                                            as u16)
                                                            .clamp(0, height as u16),
                                                    )
                                                        .unwrap();
                                                    finger_down_count = Instant::now();
                                                    //dbg!(position.x as u16-x_padding as u16,position.y as u16-y_padding as u16);
                                                }
                                            }
                                            finger_down_count = Instant::now();
                                        }
                                        FingerStatus::Motion => {
                                            if *finger_down && !disable_touch && !panning {
                                                *dragging = true;
                                            }
                                            if scale && !disable_touch {
                                                if panning {

                                                } else {
                                                     vnc.as_mut().unwrap().send_pointer_event(
                                                        0x01,
                                                        (((position.x as f32 - x_padding as f32)
                                                            / scale_factor)
                                                            as u16)
                                                            .clamp(0, width as u16),
                                                        (((position.y as f32 - y_padding as f32)
                                                            / scale_factor)
                                                            as u16)
                                                            .clamp(0, height as u16),
                                                    )
                                                        .unwrap();
                                                    //dbg!((((position.x as f32 - x_padding as f32) / scale_factor) as u16).clamp(0,width as u16), (((position.y as f32 - y_padding as f32) / scale_factor) as u16).clamp(0,height as u16));
                                                    //100-10/2 45 100/2-10=40
                                                    //from physical framebuffer means must minus padding before scale, scale is so original
                                                }
                                            } else if !disable_touch {
                                                if panning {

                                                } else {
                                                     vnc.as_mut().unwrap().send_pointer_event(
                                                        0x01,
                                                        ((position.x as i16 - x_padding as i16
                                                            + x_offset as i16)
                                                            as u16)
                                                            .clamp(0, width as u16),
                                                        ((position.y as i16 - y_padding as i16
                                                            + y_offset as i16)
                                                            as u16)
                                                            .clamp(0, height as u16),
                                                    )
                                                        .unwrap();
                                                    //dbg!(position.x as u16-x_padding as u16, position.y as u16-y_padding as u16)
                                                }
                                            }
                                        }
                                    }
                        }
                        DeviceEvent::Button { code, status, .. }
                            if code == ButtonCode::Backward || code == ButtonCode::Forward =>
                        {
                            // Upper button wheels up, lower button wheels down, one click.
                            if !paged_this_batch
                                && status != ButtonStatus::Released
                                && !*finger_down
                            {
                                send_page_scroll(
                                    vnc,
                                    code == ButtonCode::Backward,
                                    scale,
                                    width,
                                    height,
                                    fb_width,
                                    fb_height,
                                    x_offset,
                                    y_offset,
                                );
                                page_scroll_until = Some(Instant::now());
                                paged_this_batch = true;
                            }
                        }
                        DeviceEvent::Button {
                            code: ButtonCode::Power,
                            status: ButtonStatus::Pressed,
                            ..
                        } => {
                            // println!("BUTTON");
                            return event_params {
                                has_drawn_once,
                                finger_down_count,
                                exit_to_nickel: if gui_enabled {false} else {true},
                                exit_to_gui: if gui_enabled {true} else {false},
                                x_offset,
                                y_offset,
                                drag_ended,
                                page_scroll_until,
                            }
                        }
                        DeviceEvent::CoverOn => {
                            // println!("COVER");
                            return event_params {
                                has_drawn_once,
                                finger_down_count,
                                exit_to_nickel: if gui_enabled {false} else {true},
                                exit_to_gui: if gui_enabled {true} else {false},
                                x_offset,
                                y_offset,
                                drag_ended,
                                page_scroll_until,
                            }
                        }
                        // DeviceEvent::Button { code: ButtonCode::Light, status: ButtonStatus::Pressed, .. } => {
                        //     tx.send(Event::ToggleFrontlight).ok();
                        // },
                        // DeviceEvent::RotateScreen(i8) => {
                        // },
                        _ => {}
                    }
                }
                Event::Gesture(ge) => {
                    if panning {
                        match ge {
                            GestureEvent::Swipe { dir, .. } => {
                                // dbg!(x_offset, y_offset, dir);
                                match dir {
                                    Dir::North => {
                                        if height > fb_height as u16 {
                                            has_drawn_once = false;
                                            if y_offset + fb_height + fb_height / 2 < height as u32
                                            {
                                                y_offset += fb_height / 2;
                                            } else {
                                                y_offset += height as u32 - fb_height - y_offset;
                                            }
                                        }
                                    }
                                    Dir::East => {
                                        if width > fb_width as u16 {
                                            has_drawn_once = false;
                                            //0-1920-379 >379
                                            if x_offset > fb_width {
                                                x_offset -= fb_width / 2;
                                            } else {
                                                x_offset = 0;
                                            }
                                        }
                                    }
                                    Dir::South => {
                                        if height > fb_height as u16 {
                                            has_drawn_once = false;
                                            // 0 -1080/2 >1080/2
                                            if y_offset > fb_height {
                                                y_offset -= fb_height / 2;
                                            } else {
                                                y_offset = 0;
                                            }
                                        }
                                    }
                                    Dir::West => {
                                        if width > fb_width as u16 {
                                            has_drawn_once = false;
                                            //0+758+758/2 < 1920
                                            if x_offset + fb_width + fb_width / 2 < width as u32 {
                                                x_offset += fb_width / 2;
                                            } else {
                                                // =1000+1920-758-1000
                                                x_offset += width as u32 - fb_width - x_offset;
                                            }
                                        }
                                    }
                                }
                                if  vnc.as_mut().unwrap()
                                    .request_update(
                                        Rect {
                                            left: 0 + x_offset as u16,
                                            top: 0 + y_offset as u16,
                                            width: fb_width as u16,
                                            height: fb_height as u16,
                                        },
                                        false,
                                    )
                                    .is_err()
                                {
                                    error!("server disconnected");
                                    // break;
                                }
                                // fb.update(&device_fb_rect, full_update_mode).ok();
                                // dbg!(x_offset,y_offset);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {},
            };
        }
        event_params {
            has_drawn_once,
            finger_down_count,
            exit_to_nickel: false,
            exit_to_gui: false,
            x_offset,
            y_offset,
            drag_ended,
            page_scroll_until,
        }
    }
}

/// RFB pointer bits 3 and 4 are wheel up and wheel down.
const POINTER_WHEEL_UP: u8 = 1 << 3;
const POINTER_WHEEL_DOWN: u8 = 1 << 4;
/// One Kobo button press is one wheel notch. A notch has no smaller step.
const PAGE_SCROLL_CLICKS: u32 = 1;

struct PageWheel {
    x: u16,
    y: u16,
    notches: u32,
    button: u8,
}

fn visible_remote(
    scale: bool,
    width: u16,
    height: u16,
    fb_width: u32,
    fb_height: u32,
    x_offset: u32,
    y_offset: u32,
) -> (u16, u16, u16, u16) {
    if width == 0 || height == 0 {
        return (0, 0, 0, 0);
    }
    if scale {
        return (0, 0, width, height);
    }
    let vis_w = (width as u32).min(fb_width) as u16;
    let vis_h = (height as u32).min(fb_height) as u16;
    let left = x_offset.min(width.saturating_sub(vis_w) as u32) as u16;
    let top = y_offset.min(height.saturating_sub(vis_h) as u32) as u16;
    (left, top, vis_w, vis_h)
}

/// `scroll_up` is wheel up (RFB button 4). The click is aimed at the middle of the page.
fn page_wheel(scroll_up: bool, left: u16, top: u16, vis_w: u16, vis_h: u16) -> Option<PageWheel> {
    if vis_w < 8 || vis_h < 40 {
        return None;
    }
    Some(PageWheel {
        x: (left as u32 + vis_w as u32 / 2) as u16,
        y: (top as u32 + vis_h as u32 / 2) as u16,
        notches: PAGE_SCROLL_CLICKS,
        button: if scroll_up { POINTER_WHEEL_UP } else { POINTER_WHEEL_DOWN },
    })
}

/// One notch, pressed and released at the middle of the page.
fn send_page_scroll(
    vnc: &mut Option<Client>,
    scroll_up: bool,
    scale: bool,
    width: u16,
    height: u16,
    fb_width: u32,
    fb_height: u32,
    x_offset: u32,
    y_offset: u32,
) {
    let (left, top, vis_w, vis_h) =
        visible_remote(scale, width, height, fb_width, fb_height, x_offset, y_offset);
    let Some(wheel) = page_wheel(scroll_up, left, top, vis_w, vis_h) else {
        return;
    };
    let Some(client) = vnc.as_mut() else {
        return;
    };
    for _ in 0..wheel.notches {
        if client.send_pointer_event(wheel.button, wheel.x, wheel.y).is_err()
            || client.send_pointer_event(0, wheel.x, wheel.y).is_err()
        {
            error!("server disconnected");
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_wheel_is_one_click() {
        let up = page_wheel(true, 0, 0, 1000, 2000).unwrap();
        assert_eq!(up.x, 500);
        assert_eq!(up.y, 1000);
        assert_eq!(up.notches, 1);
        assert_eq!(up.button, POINTER_WHEEL_UP);

        let down = page_wheel(false, 0, 0, 1000, 2000).unwrap();
        assert_eq!(down.button, POINTER_WHEEL_DOWN);
        assert_eq!(down.notches, 1);

        assert!(page_wheel(true, 0, 0, 1000, 20).is_none());
    }

    #[test]
    fn scaled_session_scrolls_the_whole_remote() {
        assert_eq!(visible_remote(true, 1080, 2400, 1264, 1680, 100, 200), (0, 0, 1080, 2400));
        assert_eq!(visible_remote(false, 1080, 2400, 1000, 800, 50, 90), (50, 90, 1000, 800));
    }
}
