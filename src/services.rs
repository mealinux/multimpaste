//! The right-click entry.
//!
//! macOS is the only desktop with a supported way for one app to add an item to the
//! context menu of every other app: a system Service. Windows and Linux draw that menu
//! inside each application, so there the global shortcut and the tray icon are the way in.

/// Called when the user picks "Multim Paste" from a context menu. It runs on a
/// system thread, so it must only wake the app up, never touch its state.
pub type Wake = Box<dyn Fn() + Send + Sync + 'static>;

#[cfg(target_os = "macos")]
mod platform {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{AllocAnyThread, MainThreadMarker, define_class, msg_send};
    use objc2_app_kit::{NSApplication, NSUpdateDynamicServices};
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
    use std::sync::OnceLock;

    static TRIGGER: OnceLock<super::Wake> = OnceLock::new();

    define_class!(
        // SAFETY: NSObject has no subclassing requirements, and this type has no ivars
        // and no Drop implementation.
        #[unsafe(super(NSObject))]
        #[name = "MultimPasteServiceProvider"]
        struct ServiceProvider;

        impl ServiceProvider {
            /// Invoked by macOS when the Service is chosen. The picker cannot open
            /// synchronously here — the UI needs this thread — so this only rings the
            /// bell and returns, leaving the pasteboard untouched.
            #[unsafe(method(multimPaste:userData:error:))]
            fn multi_paste(
                &self,
                _pasteboard: *mut AnyObject,
                _user_data: *mut AnyObject,
                _error: *mut AnyObject,
            ) {
                if let Some(wake) = TRIGGER.get() {
                    wake();
                }
            }
        }
    );

    pub fn install(wake: impl Fn() + Send + Sync + 'static) {
        let _ = TRIGGER.set(Box::new(wake));

        let Some(mtm) = MainThreadMarker::new() else {
            eprintln!("multimpaste: services must be installed from the main thread");
            return;
        };

        let provider: Retained<ServiceProvider> =
            unsafe { msg_send![ServiceProvider::alloc(), init] };
        let app = NSApplication::sharedApplication(mtm);
        // SAFETY: the provider outlives the application, see the leak below.
        unsafe { app.setServicesProvider(Some(&provider)) };
        std::mem::forget(provider);

        // Without this the new Service only shows up after a login or a rescan.
        NSUpdateDynamicServices();
        stay_awake();
    }

    /// App Nap throttles the timers of a background app with no visible window, which
    /// showed up as the picker taking up to a second to appear. The activity is never
    /// ended, so it lasts as long as the process; idle system sleep stays allowed, so
    /// this does not keep the machine awake.
    fn stay_awake() {
        let reason = NSString::from_str("answering the clipboard shortcut without delay");
        let token = NSProcessInfo::processInfo().beginActivityWithOptions_reason(
            NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
            &reason,
        );
        std::mem::forget(token);
    }

    // In ApplicationServices, linked in through AppKit.
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    /// Whether the app may post keystrokes, asked without opening the system prompt.
    pub fn can_paste() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    /// A menu bar app is an "accessory" app: showing a window does not make it the
    /// active application, so without this the picker appears without keyboard focus.
    ///
    pub fn focus_app() {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        // Deprecated in favour of -activate, which only exists on macOS 14+ and does
        // not take focus away from the frontmost app. This one works everywhere.
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }

    /// Hand the keyboard back to whatever the user was typing in, so the paste
    /// keystroke lands there and not on us.
    ///
    /// Deactivating, not hiding: macOS stops delivering events to a hidden
    /// application, which freezes the event loop this app polls the tray, the
    /// shortcut and the Service through -- the picker would open exactly once.
    pub fn release_focus() {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        NSApplication::sharedApplication(mtm).deactivate();
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    pub fn install(_wake: impl Fn() + Send + Sync + 'static) {}

    /// Other platforms hand focus to a window when it is shown.
    pub fn focus_app() {}

    /// Only macOS gates synthetic keystrokes behind a permission.
    pub fn can_paste() -> bool {
        true
    }

    pub fn release_focus() {}
}

pub use platform::{can_paste, focus_app, install, release_focus};
