# Lens Window — Design Invariants

> **GTK3→GTK4 migration reference consolidated at the [GTK-Migrations wiki page](https://github.com/CodeMonkeyNinja/lenzu/wiki/GTK-Migrations).**
> This document keeps per-invariant GTK3/4 implementation notes as architectural
> documentation; the standalone migration reference lives there.

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

## Invariant 2 — Click-through for ordinary events; Shift+Button1 consumed by Lenzu

**What must be true:**  
- Ordinary pointer events (plain clicks, motion, scroll) pass through the lens window
  to whatever is below — the user can interact with the application under the lens.
- `Shift+Button1` (and `Ctrl+Shift+Button1`) are **consumed by Lenzu** via an X11
  passive button grab (`XGrabButton`) and must **not** reach the underlying application.

**Why:**  
The lens is a transparent overlay — it must not block normal mouse use.  But
`Shift+Button1` is Lenzu's capture hotkey; letting it fall through to the browser
causes the browser to treat it as a normal Shift+Click (e.g. opens a new tab),
while Lenzu simultaneously tries to OCR.  The user sees the browser react but no
OCR result — the UI is confusing.  Consuming the event at the X11 level before it
reaches the browser removes the ambiguity.

**Click-through (base state):**

GTK3: `window.input_shape_combine_region(None)` removes the input region.  
GTK4: `surface.set_input_region(Some(&empty_cairo_region))` — empty region = no events.

**Consuming Shift+Button1 via XGrabButton (both GTK3 and GTK4):**

Register passive button grabs on the root window, mirroring the `XGrabKey` pattern
used for keyboard combos (Invariant 5).  Must be registered for all lock-modifier
combinations (none, CapsLock, NumLock, both) so the grab fires regardless of lock state.

```rust
for &lock in &[0u16, LOCK, MOD2, LOCK|MOD2] {
    conn.grab_button(false, root,
        EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
        GrabMode::ASYNC, GrabMode::ASYNC,
        x11rb::NONE, x11rb::NONE,
        ButtonIndex::M1,
        ModMask::from(SHIFT | lock));
    // also Ctrl+Shift+Button1 for force-remote path
    conn.grab_button(false, root,
        EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
        GrabMode::ASYNC, GrabMode::ASYNC,
        x11rb::NONE, x11rb::NONE,
        ButtonIndex::M1,
        ModMask::from(CTRL | SHIFT | lock));
}
conn.flush();
```

`GrabMode::ASYNC` is non-freezing: the X server does not block other events while the
grab is active, but `ButtonPress` for `Shift+Button1` is delivered **only** to our
x11rb connection — the browser never sees it.

Detect the event in the 16ms timer by draining `Event::ButtonPress` from the x11rb
connection buffer alongside `Event::KeyPress`.  This replaces the former
`query_pointer`-mask-based `is_shift_click` detection.

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

## Invariant 5 — Key combos via X11 passive key grabs, not GDK EventController

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

**Solution — X11 passive key grabs (replaces previous `XQueryKeymap` polling):**

X11 passive key grabs (`XGrabKey`) on the root window deliver `KeyPress` events
to our x11rb connection even without WM focus. Events are polled from the connection
buffer in the 16ms timer with rising-edge detection and 200ms debounce. Zero round-trip
overhead compared to the previous `query_keymap()` polling.

See the [GTK-Migrations wiki page](https://github.com/CodeMonkeyNinja/lenzu/wiki/GTK-Migrations) § Key Combo Architecture for full details.

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
| Key combos | GDK root event filter or `query_keymap` | X11 passive key grabs (`XGrabKey`), polled from x11rb connection |
| Init order | set hints → `show()` | `realize()` → set attrs → `present()` |

**Full GTK3→GTK4 migration reference:** [GTK-Migrations wiki page](https://github.com/CodeMonkeyNinja/lenzu/wiki/GTK-Migrations)
