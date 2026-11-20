#[cfg(target_os = "macos")]
mod macos {
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSView, NSWindow, NSWindowCollectionBehavior,
    };
    use std::ffi::c_void;
    use std::ptr;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window as WinitWindow;

    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreate(source: *const c_void) -> *mut c_void;
        fn CGEventGetLocation(event: *const c_void) -> CGPoint;
        fn CFRelease(cf: *const c_void);
    }

    pub fn cursor_position() -> Option<(f64, f64)> {
        unsafe {
            let event = CGEventCreate(ptr::null());
            if event.is_null() {
                return None;
            }
            let point = CGEventGetLocation(event);
            CFRelease(event);
            Some((point.x, point.y))
        }
    }

    /// AppKit flags that make a window belong to every macOS Space, including
    /// AeroSpace-managed workspaces and full-screen Spaces.
    fn all_workspaces_behavior() -> NSWindowCollectionBehavior {
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle
            | NSWindowCollectionBehavior::FullScreenAuxiliary
    }

    /// Applies the all-Spaces behaviour to the companion's native window.
    ///
    /// Bevy/winit have no portable field for this, so we follow the raw AppKit
    /// handle from the winit view to its `NSWindow`. It is safe to call
    /// repeatedly: window managers can reset these flags when Spaces change.
    pub fn configure_all_workspaces(window: &WinitWindow) -> bool {
        let Some(ns_window) = ns_window(window) else {
            return false;
        };

        ns_window.setCollectionBehavior(all_workspaces_behavior());
        // A desktop companion should not vanish when another app is activated
        // or when AeroSpace rotates workspaces.
        ns_window.setCanHide(false);
        ns_window.setHidesOnDeactivate(false);
        ns_window.setExcludedFromWindowsMenu(true);
        true
    }

    fn ns_window(window: &WinitWindow) -> Option<Retained<NSWindow>> {
        let Ok(handle) = window.window_handle() else {
            return None;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        let ns_view: &NSView = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
        ns_view.window()
    }

    /// AeroSpace only manages apps with an `.Regular` activation policy.
    /// Accessory apps are ignored, so their windows are never moved offscreen
    /// when AeroSpace rotates its emulated workspaces.
    fn ghost_activation_policy() -> NSApplicationActivationPolicy {
        NSApplicationActivationPolicy::Accessory
    }

    /// Keeps the process invisible to AeroSpace's window manager while its
    /// window remains visible on screen.
    ///
    /// winit sets unbundled apps to `.Regular` during
    /// `applicationDidFinishLaunching`, so this is deliberately safe to call
    /// repeatedly from the app schedule.
    pub fn enforce_ghost_activation_policy() -> bool {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(ghost_activation_policy());
        true
    }

    /// Sets native window transparency independently of Bevy's render alpha.
    /// A low value keeps the companion visible but unobtrusive in ghost mode.
    pub fn set_window_alpha(window: &WinitWindow, alpha: f32) -> bool {
        let Some(ns_window) = ns_window(window) else {
            return false;
        };
        ns_window.setAlphaValue(alpha.clamp(0.0, 1.0) as _);
        true
    }

    #[cfg(test)]
    mod macos_tests {
        use super::*;

        #[test]
        fn all_spaces_behavior_includes_the_required_appkit_flags() {
            let behavior = all_workspaces_behavior();
            assert!(behavior.contains(NSWindowCollectionBehavior::CanJoinAllSpaces));
            assert!(behavior.contains(NSWindowCollectionBehavior::Stationary));
            assert!(behavior.contains(NSWindowCollectionBehavior::IgnoresCycle));
            assert!(behavior.contains(NSWindowCollectionBehavior::FullScreenAuxiliary));
        }

        #[test]
        fn ghost_policy_is_accessory_so_aerospace_ignores_the_window() {
            assert_eq!(
                ghost_activation_policy(),
                NSApplicationActivationPolicy::Accessory
            );
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::{
    configure_all_workspaces, cursor_position, enforce_ghost_activation_policy, set_window_alpha,
};

#[cfg(not(target_os = "macos"))]
pub fn configure_all_workspaces(_window: &winit::window::Window) -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn enforce_ghost_activation_policy() -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn set_window_alpha(_window: &winit::window::Window, _alpha: f32) -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn cursor_position() -> Option<(f64, f64)> {
    None
}

/// Pure check of the environment signal used to select the Wayland fallback.
fn is_wayland_session(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

/// True when this process was started in a Wayland session.
///
/// `WAYLAND_DISPLAY` is set by Wayland compositors and is not set on X11 or
/// macOS. Reading it once at startup is cheap and needs no new dependency.
pub fn is_wayland() -> bool {
    is_wayland_session(std::env::var_os("WAYLAND_DISPLAY").as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn wayland_display_selects_the_fallback() {
        assert!(is_wayland_session(Some(OsStr::new("wayland-0"))));
    }

    #[test]
    fn an_unset_or_empty_wayland_display_is_not_a_wayland_session() {
        assert!(!is_wayland_session(None));
        assert!(!is_wayland_session(Some(OsStr::new(""))));
    }
}
