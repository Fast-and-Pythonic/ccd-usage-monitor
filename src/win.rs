//! Windows-only surgery on the widget's own window.
//!
//! A frameless window on Windows does not come out frameless. `egui-winit`
//! turns on winit's undecorated-shadow mode for any window built with
//! `with_decorations(false)`:
//!
//! ```text
//! window_attributes.with_undecorated_shadow(!decorations.unwrap_or(true))
//! ```
//!
//! winit implements that by keeping `WS_CAPTION | WS_THICKFRAME` on the window
//! and collapsing the non-client area from `WM_NCCALCSIZE`, which is what makes
//! DWM draw its rectangular drop shadow — plus, in winit's own words, "a small
//! black 1px border on the top". On a translucent pill both are plainly
//! visible, and there is no `ViewportCommand` that turns the shadow off without
//! also bringing back the title bar.
//!
//! So we reach for the window handle. Nothing here is reversible from egui's
//! side, and winit rebuilds the window style whenever it changes visibility or
//! click-through, so `strip` is written to be safe to call again.

#![cfg(windows)]

use std::sync::atomic::{AtomicIsize, Ordering};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_STYLE, GWL_WNDPROC,
    SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, WM_ERASEBKGND,
    WM_NCACTIVATE, WM_NCCALCSIZE, WM_NCPAINT, WNDPROC, WS_CAPTION, WS_THICKFRAME,
};

/// The window procedure winit installed, kept so everything we do not handle
/// still reaches it. One window is subclassed, so one slot is enough.
static ORIGINAL_PROC: AtomicIsize = AtomicIsize::new(0);

/// Answer `WM_NCCALCSIZE` with "the client area is the whole window".
///
/// winit's own handler adds a pixel to the top of the client rectangle while
/// its undecorated-shadow flag is set, and the system paints that one-pixel
/// non-client strip in the frame colour — a white line across the top of a
/// window that is supposed to be invisible. The flag is internal to winit and
/// survives removing the window styles, so the message is answered before
/// winit ever sees it.
unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let mut lp = lp;
    match msg {
        // Returning zero without touching the rectangles leaves the proposed
        // window rectangle as the client rectangle.
        WM_NCCALCSIZE if wp != 0 => return 0,
        // Nothing to paint outside the client area, and nothing to erase inside
        // it: both would be drawn by the system in the frame colour, which on a
        // frameless window shows up as a white flash.
        WM_NCPAINT => return 0,
        WM_ERASEBKGND => return 1,
        // Activation changes still have to reach the default handling — the
        // move loop that drags the window depends on it. `-1` is the documented
        // way to keep it from repainting the caption while it does the rest.
        WM_NCACTIVATE => lp = -1,
        _ => {}
    }
    let original = ORIGINAL_PROC.load(Ordering::Relaxed);
    let original: WNDPROC = if original == 0 {
        None
    } else {
        Some(unsafe { std::mem::transmute::<isize, unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>(original) })
    };
    unsafe { CallWindowProcW(original, hwnd, msg, wp, lp) }
}

/// Ask Windows to draw this process's menus dark.
///
/// The tray icon's menu is a real `HMENU` shown with `TrackPopupMenu`, drawn
/// by the system rather than by us: `tray-icon` hands it to Windows and gets
/// back the light menu every process gets by default. There is no supported
/// way to change that. The switch is three functions exported from
/// `uxtheme.dll` **by ordinal only** — they have no names in the export table
/// and no headers — so they are fetched with `GetProcAddress` on an integer
/// and called through a hand-written signature:
///
/// * 135 `SetPreferredAppMode(mode)` — what this process prefers. `ForceDark`
///   rather than `AllowDark`, because the widget is dark whatever the system
///   theme is, and a light menu hanging off a dark widget is the thing being
///   fixed here.
/// * 104 `RefreshImmersiveColorPolicyState()` — re-reads the theme registry.
/// * 136 `FlushMenuThemes()` — drops uxtheme's cached menu drawing data, which
///   is what makes the *next* popup pick the preference up.
///
/// Undocumented means it can stop working; it must never be able to stop the
/// widget. Every step is optional here: a missing DLL or a missing ordinal
/// leaves the menu light and nothing else happens.
pub fn dark_menus() {
    const REFRESH_IMMERSIVE_COLOR_POLICY_STATE: usize = 104;
    const SET_PREFERRED_APP_MODE: usize = 135;
    const FLUSH_MENU_THEMES: usize = 136;
    /// `PreferredAppMode::ForceDark`.
    const FORCE_DARK: i32 = 2;

    type SetMode = unsafe extern "system" fn(i32) -> i32;
    type Nullary = unsafe extern "system" fn();

    unsafe {
        let name: Vec<u16> = "uxtheme.dll\0".encode_utf16().collect();
        let dll = LoadLibraryW(name.as_ptr());
        if dll.is_null() {
            return;
        }
        // An ordinal is passed where a name would go, as a small integer cast
        // to a pointer. That is what the MAKEINTRESOURCE macro does in C.
        let by_ordinal = |ord: usize| GetProcAddress(dll, ord as *const u8);

        if let Some(f) = by_ordinal(SET_PREFERRED_APP_MODE) {
            let set: SetMode = std::mem::transmute(f);
            set(FORCE_DARK);
        }
        if let Some(f) = by_ordinal(REFRESH_IMMERSIVE_COLOR_POLICY_STATE) {
            let refresh: Nullary = std::mem::transmute(f);
            refresh();
        }
        if let Some(f) = by_ordinal(FLUSH_MENU_THEMES) {
            let flush: Nullary = std::mem::transmute(f);
            flush();
        }
    }
}

/// The widget's window, addressed the way Win32 wants it.
#[derive(Clone, Copy)]
pub struct Frame {
    hwnd: isize,
}

impl Frame {
    pub fn from(handle: &impl HasWindowHandle) -> Option<Frame> {
        match handle.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => Some(Frame {
                hwnd: h.hwnd.get(),
            }),
            _ => None,
        }
    }

    /// Take over the window procedure so the non-client area is never carved
    /// out of the window at all. Called once, right after the window exists.
    pub fn subclass(self) {
        let hwnd = self.hwnd as HWND;
        unsafe {
            let previous = SetWindowLongPtrW(hwnd, GWL_WNDPROC, wnd_proc as *const () as isize);
            if previous != 0 {
                ORIGINAL_PROC.store(previous, Ordering::Relaxed);
            }
        }
    }

    /// Remove the drop shadow and the non-client frame that comes with it.
    ///
    /// Two calls, and both are needed: the DWM attribute stops the shadow being
    /// drawn, the style change removes the frame that the shadow hangs off.
    /// The one-pixel line along the top is the subclass's job.
    pub fn strip(self) {
        let hwnd = self.hwnd as HWND;
        unsafe {
            let policy = DWMNCRP_DISABLED;
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_NCRENDERING_POLICY as u32,
                std::ptr::addr_of!(policy).cast(),
                std::mem::size_of_val(&policy) as u32,
            );

            let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            let wanted = style & !((WS_CAPTION | WS_THICKFRAME) as isize);
            if wanted != style {
                SetWindowLongPtrW(hwnd, GWL_STYLE, wanted);
            }
            // Even when the style was already right, the frame has to be
            // recalculated for the DWM attribute to take effect on a window
            // that is already on screen.
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER,
            );
        }
    }
}
