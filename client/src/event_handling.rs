use std::sync::mpsc::{self, Sender, Receiver};
use std::thread;
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
                            // Upper button scrolls up, lower button scrolls down.
                            // One finger stroke, about five former wheel clicks.
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

/// One press moves about five of the old wheel clicks (5 × 160px), in one stroke.
const PAGE_SCROLL_CLICKS: u32 = 5;
const WHEEL_PX_PER_NOTCH: u32 = 160;
const STROKE_GAP_MS: u64 = 16;
/// Hold still before the lift. A moving lift becomes a fling and the page
/// keeps scrolling after the finger is gone.
const STROKE_STOP_MS: u64 = 50;

struct PageStroke {
    x: u16,
    start_y: u16,
    end_y: u16,
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

/// `scroll_up` drags the finger down, which pulls earlier content into view.
fn page_stroke(scroll_up: bool, left: u16, top: u16, vis_w: u16, vis_h: u16) -> Option<PageStroke> {
    if vis_w < 8 || vis_h < 40 {
        return None;
    }
    let margin = 24u32.max(vis_h as u32 / 10);
    let max_travel = (vis_h as u32).saturating_sub(margin.saturating_mul(2));
    if max_travel < 8 {
        return None;
    }
    let travel = (PAGE_SCROLL_CLICKS * WHEEL_PX_PER_NOTCH).min(max_travel);
    let x = left as u32 + vis_w as u32 / 2;
    let y_top = top as u32 + (vis_h as u32 - travel) / 2;
    let y_bot = y_top + travel;
    let (start_y, end_y) = if scroll_up {
        (y_top, y_bot)
    } else {
        (y_bot, y_top)
    };
    Some(PageStroke {
        x: x as u16,
        start_y: start_y as u16,
        end_y: end_y as u16,
    })
}

/// One gesture: finger down, one move of the page distance, then a stop.
/// The finger stays still before it lifts so the remote does not fling onward.
/// A VNC wheel event has no distance field, so a single click cannot stand in
/// for several notches.
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
    let Some(stroke) = page_stroke(scroll_up, left, top, vis_w, vis_h) else {
        return;
    };
    let Some(client) = vnc.as_mut() else {
        return;
    };
    let mut pointer = |mask: u8, y: u16| -> bool {
        client.send_pointer_event(mask, stroke.x, y).is_ok()
    };
    if !pointer(1, stroke.start_y) {
        error!("server disconnected");
        return;
    }
    thread::sleep(Duration::from_millis(STROKE_GAP_MS));
    if !pointer(1, stroke.end_y) {
        error!("server disconnected");
        return;
    }
    // Two stationary samples, about 100ms, then lift. That is the stop.
    thread::sleep(Duration::from_millis(STROKE_STOP_MS));
    if !pointer(1, stroke.end_y) {
        error!("server disconnected");
        return;
    }
    thread::sleep(Duration::from_millis(STROKE_STOP_MS));
    if !pointer(0, stroke.end_y) {
        error!("server disconnected");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_stroke_matches_about_five_wheel_clicks() {
        let up = page_stroke(true, 0, 0, 1000, 2000).unwrap();
        assert_eq!(up.x, 500);
        assert!(up.start_y < up.end_y);
        assert_eq!(up.end_y - up.start_y, 800);

        let down = page_stroke(false, 0, 0, 1000, 2000).unwrap();
        assert!(down.start_y > down.end_y);
        assert_eq!(down.start_y - down.end_y, 800);

        assert!(page_stroke(true, 0, 0, 1000, 20).is_none());
    }

    #[test]
    fn scaled_session_scrolls_the_whole_remote() {
        assert_eq!(visible_remote(true, 1080, 2400, 1264, 1680, 100, 200), (0, 0, 1080, 2400));
        assert_eq!(visible_remote(false, 1080, 2400, 1000, 800, 50, 90), (50, 90, 1000, 800));
    }
}
