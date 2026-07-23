//! Translates GTK input events into [`beam_core::session::InputEvent`]s.
//!
//! Keyboard handling sends PS/2 Scan Code Set 1 bytes, not keysyms, so the remote Windows
//! machine's own keyboard layout (including ABNT2) does the actual character interpretation —
//! Beam never needs to know what layout the user has configured. GDK reports hardware keycodes
//! using the historical X11 convention of `evdev_code + 8` on both the X11 and Wayland backends,
//! so subtracting 8 recovers the raw Linux evdev keycode. For the base 84-key block that raw
//! evdev code is numerically identical to its Scan Code Set 1 byte (both trace back to the
//! original AT keyboard controller encoding); a fixed table below covers the keys added later
//! (arrows, Ins/Del/Home/End/PgUp/PgDn, the numeric-keypad Enter, right Ctrl, right Alt/AltGr)
//! whose evdev codes don't follow that arithmetic and need the E0 "extended" prefix.

use std::cell::Cell;
use std::rc::Rc;

use beam_core::session::{InputEvent, PointerButton};
use gtk::glib;
use gtk::prelude::*;

/// `(evdev code, Set 1 scancode byte, needs E0 extended prefix)` for keys whose evdev code does
/// not equal its Set 1 byte.
const EXTENDED_KEYS: &[(u32, u8, bool)] = &[
    (96, 0x1C, true),  // KP_Enter
    (97, 0x1D, true),  // Right Ctrl
    (98, 0x35, true),  // KP_Slash
    (100, 0x38, true), // Right Alt / AltGr
    (102, 0x47, true), // Home
    (103, 0x48, true), // Up
    (104, 0x49, true), // Page Up
    (105, 0x4B, true), // Left
    (106, 0x4D, true), // Right
    (107, 0x4F, true), // End
    (108, 0x50, true), // Down
    (109, 0x51, true), // Page Down
    (110, 0x52, true), // Insert
    (111, 0x53, true), // Delete
    (125, 0x5B, true), // Left Super/Meta
    (126, 0x5C, true), // Right Super/Meta
    (127, 0x5D, true), // Menu
];

/// Convert a GDK hardware keycode into a `(scancode, extended)` pair, or `None` if the key has
/// no PS/2 Scan Code Set 1 equivalent we know how to send (e.g. multimedia keys).
fn keycode_to_scancode(gdk_keycode: u32) -> Option<(u8, bool)> {
    let evdev = gdk_keycode.checked_sub(8)?;

    if let Some(&(_, scancode, extended)) = EXTENDED_KEYS.iter().find(|&&(code, _, _)| code == evdev) {
        return Some((scancode, extended));
    }

    // The base AT-84 block: Esc(1) .. F12(88), skipping nothing we care about here.
    if (1..=88).contains(&evdev) {
        return Some((evdev as u8, false));
    }

    None
}

/// Wire mouse and keyboard controllers on `widget`, translating events through `to_remote`
/// (widget-local coordinates → remote desktop coordinates) and dispatching via `send`.
pub fn attach<F, S>(widget: &impl IsA<gtk::Widget>, to_remote: F, send: S)
where
    F: Fn(f64, f64) -> Option<(u16, u16)> + Clone + 'static,
    S: Fn(InputEvent) + Clone + 'static,
{
    let widget = widget.clone().upcast::<gtk::Widget>();
    let last_pos: Rc<Cell<(f64, f64)>> = Rc::new(Cell::new((0.0, 0.0)));

    // Pointer motion.
    {
        let to_remote = to_remote.clone();
        let send = send.clone();
        let last_pos = last_pos.clone();
        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(move |_, x, y| {
            last_pos.set((x, y));
            if let Some((rx, ry)) = to_remote(x, y) {
                send(InputEvent::MouseMove { x: rx, y: ry });
            }
        });
        widget.add_controller(motion);
    }

    // Mouse buttons (any button; disambiguated via `current_button()`).
    {
        let to_remote_press = to_remote.clone();
        let send_press = send.clone();
        let click = gtk::GestureClick::new();
        click.set_button(0);
        click.connect_pressed(move |gesture, _n_press, x, y| {
            if let (Some(button), Some((rx, ry))) = (pointer_button(gesture.current_button()), to_remote_press(x, y)) {
                send_press(InputEvent::MouseButton {
                    x: rx,
                    y: ry,
                    button,
                    pressed: true,
                });
            }
        });
        let to_remote_release = to_remote.clone();
        let send_release = send.clone();
        click.connect_released(move |gesture, _n_press, x, y| {
            if let (Some(button), Some((rx, ry))) = (pointer_button(gesture.current_button()), to_remote_release(x, y)) {
                send_release(InputEvent::MouseButton {
                    x: rx,
                    y: ry,
                    button,
                    pressed: false,
                });
            }
        });
        widget.add_controller(click);
    }

    // Scroll wheel (vertical only, per v1 scope). `EventControllerScroll` doesn't report a
    // position, so we reuse the last position seen by the motion controller.
    {
        let to_remote = to_remote.clone();
        let send = send.clone();
        let last_pos = last_pos.clone();
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll.connect_scroll(move |_, _dx, dy| {
            // RDP wheel units are ±120 per physical notch; GDK reports `dy` in "scroll steps"
            // (typically ±1 per notch), so scale accordingly. A positive GDK delta means
            // scrolling down/away from the user, matching RDP's downward-scroll convention.
            let steps = (-dy * 120.0).round().clamp(-120.0, 120.0) as i16;
            if steps != 0 {
                let (x, y) = last_pos.get();
                if let Some((rx, ry)) = to_remote(x, y) {
                    send(InputEvent::MouseWheel { x: rx, y: ry, steps });
                }
            }
            glib::Propagation::Stop
        });
        widget.add_controller(scroll);
    }

    // Keyboard.
    {
        let send_press = send.clone();
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, _keyval, keycode, _state| {
            if let Some((scancode, extended)) = keycode_to_scancode(keycode) {
                send_press(InputEvent::Key {
                    scancode,
                    extended,
                    pressed: true,
                });
            }
            glib::Propagation::Stop
        });
        let send_release = send.clone();
        key.connect_key_released(move |_, _keyval, keycode, _state| {
            if let Some((scancode, extended)) = keycode_to_scancode(keycode) {
                send_release(InputEvent::Key {
                    scancode,
                    extended,
                    pressed: false,
                });
            }
        });
        widget.add_controller(key);
    }
}

fn pointer_button(gdk_button: u32) -> Option<PointerButton> {
    match gdk_button {
        1 => Some(PointerButton::Left),
        2 => Some(PointerButton::Middle),
        3 => Some(PointerButton::Right),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_letter_key_is_gdk_keycode_minus_eight_non_extended() {
        // 'A' is evdev KEY_A = 30, Set 1 scancode 0x1E; GDK reports evdev + 8 = 38.
        assert_eq!(keycode_to_scancode(38), Some((0x1E, false)));
    }

    #[test]
    fn escape_key_maps_to_set1_byte_one() {
        // KEY_ESC = 1, GDK keycode = 9.
        assert_eq!(keycode_to_scancode(9), Some((0x01, false)));
    }

    #[test]
    fn arrow_keys_are_extended() {
        // KEY_LEFT = 105 -> GDK 113.
        assert_eq!(keycode_to_scancode(113), Some((0x4B, true)));
        // KEY_UP = 103 -> GDK 111.
        assert_eq!(keycode_to_scancode(111), Some((0x48, true)));
    }

    #[test]
    fn right_ctrl_and_right_alt_are_extended() {
        // KEY_RIGHTCTRL = 97 -> GDK 105.
        assert_eq!(keycode_to_scancode(105), Some((0x1D, true)));
        // KEY_RIGHTALT = 100 -> GDK 108.
        assert_eq!(keycode_to_scancode(108), Some((0x38, true)));
    }

    #[test]
    fn numpad_enter_is_extended_while_main_enter_is_not() {
        // KEY_ENTER = 28 -> GDK 36 (main Enter, not extended).
        assert_eq!(keycode_to_scancode(36), Some((0x1C, false)));
        // KEY_KPENTER = 96 -> GDK 104 (numpad Enter, extended).
        assert_eq!(keycode_to_scancode(104), Some((0x1C, true)));
    }

    #[test]
    fn keycodes_below_eight_are_rejected() {
        assert_eq!(keycode_to_scancode(3), None);
    }

    #[test]
    fn pointer_button_mapping_matches_gdk_convention() {
        assert_eq!(pointer_button(1), Some(PointerButton::Left));
        assert_eq!(pointer_button(2), Some(PointerButton::Middle));
        assert_eq!(pointer_button(3), Some(PointerButton::Right));
        assert_eq!(pointer_button(8), None);
    }
}
