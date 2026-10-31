use std::mem;
use crate::device::CURRENT_DEVICE;
use crate::framebuffer::{Framebuffer, UpdateMode};
use crate::geom::{Rectangle, CornerSpec};
use super::{View, Event, Hub, Bus, Id, ID_FEEDER, RenderQueue, RenderData, EntryKind};
use crate::input::{DeviceEvent, FingerStatus};
use crate::gesture::GestureEvent;
use crate::font::{Fonts, font_from_style, NORMAL_STYLE, SPECIAL_STYLE};
use crate::color::{TEXT_NORMAL, TEXT_INVERTED_HARD};
use crate::context::Context;

pub struct MenuEntry {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    kind: EntryKind,
    corner_spec: Option<CornerSpec>,
    anchor: Rectangle,
    active: bool,
    disabled: bool,
}

impl MenuEntry {
    pub fn new(rect: Rectangle, kind: EntryKind, anchor: Rectangle, corner_spec: Option<CornerSpec>) -> MenuEntry {
        MenuEntry {
            id: ID_FEEDER.next(),
            rect,
            children: Vec::new(),
            kind,
            corner_spec,
            anchor,
            active: false,
            disabled: false,
        }
    }

    pub fn update(&mut self, value: bool, rq: &mut RenderQueue) {
        if let Some(v) = self.kind.get() {
            if v != value {
                self.kind.set(value);
                rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
            }
        }
    }

    pub fn set_disabled(&mut self, value: bool, rq: &mut RenderQueue) {
        if self.disabled == value {
            return;
        }
        self.disabled = value;
        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
    }
}

impl View for MenuEntry {
    fn handle_event(&mut self, evt: &Event, _hub: &Hub, bus: &mut Bus, rq: &mut RenderQueue, _context: &mut Context) -> bool {
        match *evt {
            Event::Device(DeviceEvent::Finger { status, position, .. }) => {
                match status {
                    FingerStatus::Down if self.rect.includes(position) => {
                        self.active = true;
                        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
                        true
                    },
                    FingerStatus::Up if self.active => {
                        self.active = false;
                        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
                        true
                    },
                    _ => false,
                }
            },
            Event::Gesture(GestureEvent::Tap(center)) |
            Event::Gesture(GestureEvent::HoldFingerShort(center, ..)) if self.rect.includes(center) && !self.disabled => {
                match self.kind {
                    EntryKind::CheckBox(_, _, ref mut value) => {
                        *value = !*value;
                        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
                    },
                    EntryKind::RadioButton(_, _, ref mut value) if !*value => {
                        *value = true;
                        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
                    },
                    _ => (),
                };
                match self.kind {
                    EntryKind::Command(_, ref id) |
                    EntryKind::CheckBox(_, ref id, _) |
                    EntryKind::RadioButton(_, ref id, _) => {
                        bus.push_back(Event::Select(id.clone()));
                        if let Event::Gesture(GestureEvent::Tap { .. }) = *evt {
                            bus.push_back(Event::Validate);
                        }
                    },
                    EntryKind::SubMenu(_, ref entries) | EntryKind::More(ref entries) => {
                        bus.push_back(Event::SubMenu(self.anchor, entries.clone()));
                    },
                    EntryKind::Message(..) => {
                        bus.push_back(Event::Validate);
                    },
                    _ => (),
                };
                true
            },
            Event::PropagateSelect(ref other_id) => {
                match self.kind {
                    EntryKind::RadioButton(_, ref id, ref mut value) if *value => {
                        if mem::discriminant(id) == mem::discriminant(other_id) && id != other_id {
                            *value = false;
                            rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
                            true
                        } else {
                            false
                        }
                    },
                    _ => false,
                }
            },
            _ => false,
        }
    }

    fn render(&self, fb: &mut dyn Framebuffer, _rect: Rectangle, fonts: &mut Fonts) {
        let dpi = CURRENT_DEVICE.dpi;
        let style = if matches!(self.kind, EntryKind::More(..)) {
            SPECIAL_STYLE
        } else {
            NORMAL_STYLE
        };
        let font = font_from_style(fonts, &style, dpi);
        let x_height = font.x_heights.0 as i32;
        let padding = 4 * font.em() as i32;

        let scheme = if self.active {
            TEXT_INVERTED_HARD
        } else {
            TEXT_NORMAL
        };
        let foreground = if self.disabled { scheme[2] } else { scheme[1] };

        if let Some(ref cs) = self.corner_spec {
            fb.draw_rounded_rectangle(&self.rect, cs, scheme[0]);
        } else {
            fb.draw_rectangle(&self.rect, scheme[0]);
        }

        let max_width = self.rect.width() as i32 - padding;
        let plan = font.plan(self.kind.text(), Some(max_width), None);
        let dy = (self.rect.height() as i32 - x_height) / 2;
        let pt = pt!(self.rect.min.x + padding / 2,
                     self.rect.max.y - dy);

        font.render(fb, foreground, &plan, pt); 

        let mark = match self.kind {
            EntryKind::CheckBox(_, _, true) => Some("x"),
            EntryKind::RadioButton(_, _, true) => Some("*"),
            EntryKind::SubMenu(..) | EntryKind::More(..) => Some(">"),
            _ => None,
        };
        if let Some(mark) = mark {
            let mark_plan = font.plan(mark, None, None);
            let dx = self.rect.width() as i32 - padding / 2 - mark_plan.width;
            font.render(fb, foreground, &mark_plan, pt!(self.rect.min.x + dx, pt.y));
        }
    }

    fn rect(&self) -> &Rectangle {
        &self.rect
    }

    fn rect_mut(&mut self) -> &mut Rectangle {
        &mut self.rect
    }

    fn children(&self) -> &Vec<Box<dyn View>> {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn View>> {
        &mut self.children
    }

    fn id(&self) -> Id {
        self.id
    }
}
