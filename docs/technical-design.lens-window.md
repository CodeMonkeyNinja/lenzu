# Lens Window — Design Invariants

This document captures the design constraints for the transparent lens overlay window.
These invariants are **toolkit-agnostic** — they must hold regardless of whether the
underlying UI toolkit is GTK3, GTK4, or anything else.  The "GTK3" and "GTK4" sections
below each document how the invariant is satisfied per toolkit.

---

## Invariant 1 — Window manager bypass (override_redirect)

**What must be true:**  
The lens window must be exempt from window-manager geometry management.
The WM must not:
- Reposition the window to keep it "on screen"
- Add decorations or title bars
- Intercept `ConfigureRequest` events from the application

**Why:**  
The lens hides by moving to `(-10000, -10000)`.  If the WM intercepts
`ConfigureRequest`, it silently rejects off-screen positions and the window stays
visible.  Result: the lens is always visible and always follows the cursor, regardless
of whether Shift is held.

**GTK3 — `WindowType::Popup`:**  
`gtk::Window::builder().type_(gtk::WindowType::Popup).build()` produces a
`GDK_WINDOW_TYPE_TEMP` surface.  GDK3 automatically sets `override_redirect = True`
on the underlying X11 window before mapping.  No explicit x11rb call needed.

**GTK4 — explicit x11rb `change_window_attributes`:**  
`ApplicationWindow` has no Popup equivalent in GTK4.  After the surface is realized but
**before** `window.present()` maps the window, call:

```rust
let cwa = ChangeWindowAttributesAux::new().override_redirect(1u32);
let _ = conn.change_window_attributes(xid, &cwa);
```

`window.realize()` (via `gtk4::prelude::WidgetExt::realize`) creates the surface and
XID without mapping the window.  This ensures override_redirect is set before the
window is ever seen by the WM.  Calling it after `present()` still works but produces
a brief visible flash at the default WM-assigned position.

Because override_redirect bypasses the WM entirely, `_NET_WM_STATE_ABOVE` ClientMessages
are irrelevant.  Use `ConfigureWindowAux::stack_mode(StackMode::ABOVE)` directly
to keep the window above all others.

---

## Invariant 2 — Click-through (empty input region)

**What must be true:**  
All pointer events (clicks, motion) pass through the lens window to whatever is below.
The lens must not capture or consume mouse input.

**Why:**  
The user must be able to interact normally with the application under the lens while
Shift is held.  Shift+Click must reach both the underlying application (to trigger its
normal action) AND be visible to Lenzu's pointer-state poller.

**GTK3 — `input_shape_combine_region`:**  
`window.input_shape_combine_region(None)` removes the input region, making the whole
window pass-through.

**GTK4 — `surface.set_input_region`:**  
`input_shape_combine_region` is not exposed in gtk4-rs.  Use the GDK4 surface directly:

```rust
let region = cairo::Region::create(); // empty region
surface.set_input_region(Some(&region));
```

Call this after `realize()` (surface must exist) and before or after `present()`;
both work.  The ShapeInput extension is applied to the X11 window directly.

---

## Invariant 3 — Window always stays mapped ("never unmap")

**What must be true:**  
The lens window is **never unmapped** (never `set_visible(false)` / `hide()`).  
It hides by moving off-screen to `(-10000, -10000)`, not by unmapping.

**Why:**  
GTK4's frame-clock only delivers frames to mapped surfaces.  If the window is unmapped
(via `set_visible(false)`) and then re-mapped, the frame clock may not resume
delivering frames to the surface, causing `queue_draw()` calls to be silently dropped.
The loading spinner stops animating during OCR; other redraws stop working.  
See `§2k` in `prototypes-desktop-issues.md` for the investigation notes.

**Off-screen hide sequence (both GTK3 and GTK4):**

```
move_window(-10000, -10000)   // window is mapped but off-screen
conn.flush()                  // must flush immediately — ConfigureRequest is
                              // buffered by x11rb until flushed or a reply is read
// ... capture screen ...
move_window(win_x, win_y)     // move back to cursor before redraw
```

`conn.flush()` is required.  Without it, the ConfigureRequest sits in x11rb's send
buffer until the next `query_pointer` reply arrives.  During the 400ms screenshot
sleep, the window has not actually moved, and the screenshot captures the lens content
instead of the underlying screen.

---

## Invariant 4 — Pointer state via XQueryPointer, not GDK events

**What must be true:**  
Shift-held detection and Shift+Click detection must use X11 `QueryPointer` polled
from a 16ms timer, not GDK event callbacks.

**Why:**  
- The lens window is click-through (Invariant 2), so it receives no pointer events
  through the normal GDK event path.
- The lens window has no WM focus (Invariant 1, override_redirect), so GDK motion
  events are not delivered.
- `QueryPointer` on the root window returns the current global pointer position and
  modifier/button mask regardless of which window has focus.

**Polling loop (GTK3 and GTK4):**

```rust
glib::timeout_add_local(Duration::from_millis(16), move || {
    let (x, y, mask) = pointer_position()?;     // conn.query_pointer(root)
    let shift_held    = (mask & SHIFT.bits())   != 0;
    let button1_down  = (mask & BUTTON1.bits()) != 0;
    let is_shift_click = shift_held && button1_down;

    if shift_held { move_window(cursor_x, cursor_y); }
    else          { move_window(-10000, -10000);      }
    // ...
});
```

The `mask` field of `QueryPointerReply` is a `KeyButMask` bitmask:
- `SHIFT   = 0x0001`
- `BUTTON1 = 0x0100`
- `CONTROL = 0x0004`

---

## Invariant 5 — Key combos via XQueryKeymap, not GDK EventController

**What must be true:**  
ESC, Shift+ESC, Shift+H, Shift+Tab must be detectable without the lens window
having keyboard focus.

**Why:**  
- With override_redirect (Invariant 1), the WM never gives the window keyboard focus.
- The lens window is click-through (Invariant 2), so the user cannot click it to
  gain focus.
- Therefore GDK `EventControllerKey` / `connect_key_pressed` will only fire at
  application startup (the brief moment before the user interacts with another window).
  After the user focuses Brave/Chrome to read Japanese text, key events stop arriving
  at the lens window entirely.

**Solution — XQueryKeymap polled in the same 16ms timer:**

```rust
fn key_pressed(keycode: u8) -> bool {
    let reply = conn.query_keymap()?.reply()?;
    (reply.keys[(keycode / 8) as usize] & (1u8 << (keycode % 8))) != 0
}
// Standard Linux/evdev X11 keycodes (stable across keyboard layouts):
const KEYCODE_ESC: u8 = 9;   // Escape (kernel 1  + 8)
const KEYCODE_H:   u8 = 43;  // H key  (kernel 35 + 8)
const KEYCODE_TAB: u8 = 23;  // Tab    (kernel 15 + 8)
```

Debounce by tracking `{key}_was_down` booleans in the timer closure and acting only on
`key_down && !key_was_down` (rising edge).

**GTK3 equivalent:**  
GTK3 had a global key event filter (`gtk_key_snooper_install`, deprecated in 3.x) or
could use a GDK event filter on the root window.  Either way, the timer-based
`XQueryKeymap` approach works on both GTK3 and GTK4 and is the preferred path going
forward because it is independent of focus and toolkit version.

---

## Invariant 6 — Initialization order

The following order is required on startup:

```
1. widget.realize()           // GTK4: creates GDK surface + X11 XID without mapping
                              // GTK3: gdk::Window is created on first show(); realize
                              //       is implicit — set WM hints before show() instead
2. set_window_state()         // set override_redirect + initial (-10000,-10000) + ABOVE
3. input_shape_clickthrough() // set empty input region (ShapeInput)
4. window.present()           // map the window (already at -10000,-10000)
```

Do **not** call `window.present()` before `set_window_state()`.  If override_redirect
is set after mapping, some compositors see the window at its WM-assigned default
position for one frame.

---

## Summary table

| Invariant | GTK3 mechanism | GTK4 mechanism |
|-----------|---------------|----------------|
| WM bypass (override_redirect) | `WindowType::Popup` auto-sets it | `change_window_attributes(xid, override_redirect=1)` via x11rb, before `present()` |
| Click-through | `input_shape_combine_region(None)` | `surface.set_input_region(Some(&empty_region))` |
| Never unmap | `move_window(-10000,-10000)` + `conn.flush()` | same |
| Pointer state | `query_pointer` polled in 16ms timer | same |
| Key combos | GDK root event filter or `query_keymap` | `query_keymap` polled in 16ms timer (GDK EventController unreliable without focus) |
| Init order | set hints → `show()` | `realize()` → set attrs → `present()` |
