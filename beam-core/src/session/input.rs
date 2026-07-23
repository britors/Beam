//! Translation from Beam's toolkit-agnostic input events into IronRDP fast-path PDUs.
//!
//! Frontends never construct `ironrdp_pdu` types directly; they send [`InputEvent`], built from
//! whatever their windowing toolkit hands them (for GTK: hardware evdev keycodes and pointer
//! coordinates already scaled to the remote desktop).

use ironrdp_pdu::input::fast_path::{FastPathInputEvent, KeyboardFlags};
use ironrdp_pdu::input::mouse::PointerFlags;
use ironrdp_pdu::input::MousePdu;
use smallvec::SmallVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    MouseMove {
        x: u16,
        y: u16,
    },
    MouseButton {
        x: u16,
        y: u16,
        button: PointerButton,
        pressed: bool,
    },
    /// `steps` is in RDP wheel units (±120 per physical notch, matching `WHEEL_DELTA`).
    MouseWheel {
        x: u16,
        y: u16,
        steps: i16,
    },
    /// A single key transition. `scancode` is the PS/2 Scan Code Set 1 byte (without the E0
    /// prefix, which is carried by `extended` instead); see [`crate::session::scancode`].
    Key {
        scancode: u8,
        extended: bool,
        pressed: bool,
    },
}

pub(crate) fn to_fastpath(event: InputEvent) -> SmallVec<[FastPathInputEvent; 2]> {
    let mut out = SmallVec::new();
    match event {
        InputEvent::MouseMove { x, y } => {
            out.push(FastPathInputEvent::MouseEvent(MousePdu {
                flags: PointerFlags::MOVE,
                number_of_wheel_rotation_units: 0,
                x_position: x,
                y_position: y,
            }));
        }
        InputEvent::MouseButton { x, y, button, pressed } => {
            let button_flag = match button {
                PointerButton::Left => PointerFlags::LEFT_BUTTON,
                PointerButton::Right => PointerFlags::RIGHT_BUTTON,
                PointerButton::Middle => PointerFlags::MIDDLE_BUTTON_OR_WHEEL,
            };
            let mut flags = button_flag;
            if pressed {
                flags |= PointerFlags::DOWN;
            }
            out.push(FastPathInputEvent::MouseEvent(MousePdu {
                flags,
                number_of_wheel_rotation_units: 0,
                x_position: x,
                y_position: y,
            }));
        }
        InputEvent::MouseWheel { x, y, steps } => {
            out.push(FastPathInputEvent::MouseEvent(MousePdu {
                flags: PointerFlags::VERTICAL_WHEEL,
                number_of_wheel_rotation_units: steps,
                x_position: x,
                y_position: y,
            }));
        }
        InputEvent::Key {
            scancode,
            extended,
            pressed,
        } => {
            let mut flags = KeyboardFlags::empty();
            if !pressed {
                flags |= KeyboardFlags::RELEASE;
            }
            if extended {
                flags |= KeyboardFlags::EXTENDED;
            }
            out.push(FastPathInputEvent::KeyboardEvent(flags, scancode));
        }
    }
    out
}

/// The four key transitions that make up the classic Secure Attention Sequence emulation:
/// Ctrl down, Alt down, Del down, Del up, Alt up, Ctrl up. RDP servers recognize this exact
/// scancode sequence and raise it as SAS, exactly like a physical Ctrl+Alt+Del would.
pub(crate) fn ctrl_alt_del_sequence() -> SmallVec<[FastPathInputEvent; 6]> {
    const LEFT_CTRL: u8 = 0x1D;
    const LEFT_ALT: u8 = 0x38;
    const DELETE: u8 = 0x53;

    let mut out = SmallVec::new();
    out.push(FastPathInputEvent::KeyboardEvent(KeyboardFlags::empty(), LEFT_CTRL));
    out.push(FastPathInputEvent::KeyboardEvent(KeyboardFlags::empty(), LEFT_ALT));
    out.push(FastPathInputEvent::KeyboardEvent(KeyboardFlags::EXTENDED, DELETE));
    out.push(FastPathInputEvent::KeyboardEvent(
        KeyboardFlags::EXTENDED | KeyboardFlags::RELEASE,
        DELETE,
    ));
    out.push(FastPathInputEvent::KeyboardEvent(KeyboardFlags::RELEASE, LEFT_ALT));
    out.push(FastPathInputEvent::KeyboardEvent(KeyboardFlags::RELEASE, LEFT_CTRL));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_move_produces_a_move_flagged_event_with_no_wheel() {
        let events = to_fastpath(InputEvent::MouseMove { x: 42, y: 7 });
        assert_eq!(events.len(), 1);
        let FastPathInputEvent::MouseEvent(pdu) = events[0] else {
            panic!("expected a mouse event");
        };
        assert_eq!(pdu.flags, PointerFlags::MOVE);
        assert_eq!(pdu.x_position, 42);
        assert_eq!(pdu.y_position, 7);
        assert_eq!(pdu.number_of_wheel_rotation_units, 0);
    }

    #[test]
    fn mouse_button_down_sets_button_and_down_flags() {
        let events = to_fastpath(InputEvent::MouseButton {
            x: 1,
            y: 2,
            button: PointerButton::Right,
            pressed: true,
        });
        let FastPathInputEvent::MouseEvent(pdu) = events[0] else {
            panic!("expected a mouse event");
        };
        assert!(pdu.flags.contains(PointerFlags::RIGHT_BUTTON));
        assert!(pdu.flags.contains(PointerFlags::DOWN));
    }

    #[test]
    fn mouse_button_up_omits_down_flag() {
        let events = to_fastpath(InputEvent::MouseButton {
            x: 1,
            y: 2,
            button: PointerButton::Left,
            pressed: false,
        });
        let FastPathInputEvent::MouseEvent(pdu) = events[0] else {
            panic!("expected a mouse event");
        };
        assert!(pdu.flags.contains(PointerFlags::LEFT_BUTTON));
        assert!(!pdu.flags.contains(PointerFlags::DOWN));
    }

    #[test]
    fn key_press_and_release_map_to_release_flag() {
        let pressed = to_fastpath(InputEvent::Key {
            scancode: 0x1E,
            extended: false,
            pressed: true,
        });
        let FastPathInputEvent::KeyboardEvent(flags, code) = pressed[0] else {
            panic!("expected a keyboard event");
        };
        assert_eq!(code, 0x1E);
        assert!(!flags.contains(KeyboardFlags::RELEASE));

        let released = to_fastpath(InputEvent::Key {
            scancode: 0x1E,
            extended: false,
            pressed: false,
        });
        let FastPathInputEvent::KeyboardEvent(flags, _) = released[0] else {
            panic!("expected a keyboard event");
        };
        assert!(flags.contains(KeyboardFlags::RELEASE));
    }

    #[test]
    fn extended_key_sets_extended_flag() {
        let events = to_fastpath(InputEvent::Key {
            scancode: 0x4B,
            extended: true,
            pressed: true,
        });
        let FastPathInputEvent::KeyboardEvent(flags, _) = events[0] else {
            panic!("expected a keyboard event");
        };
        assert!(flags.contains(KeyboardFlags::EXTENDED));
    }

    #[test]
    fn ctrl_alt_del_is_the_documented_six_event_sequence() {
        let events = ctrl_alt_del_sequence();
        assert_eq!(events.len(), 6);

        let expect_key = |event: &FastPathInputEvent, scancode: u8, released: bool, extended: bool| match event {
            FastPathInputEvent::KeyboardEvent(flags, code) => {
                assert_eq!(*code, scancode);
                assert_eq!(flags.contains(KeyboardFlags::RELEASE), released);
                assert_eq!(flags.contains(KeyboardFlags::EXTENDED), extended);
            }
            _ => panic!("expected a keyboard event"),
        };

        expect_key(&events[0], 0x1D, false, false); // Ctrl down
        expect_key(&events[1], 0x38, false, false); // Alt down
        expect_key(&events[2], 0x53, false, true); // Del down (extended)
        expect_key(&events[3], 0x53, true, true); // Del up (extended)
        expect_key(&events[4], 0x38, true, false); // Alt up
        expect_key(&events[5], 0x1D, true, false); // Ctrl up
    }
}
