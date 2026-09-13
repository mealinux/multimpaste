//! The right-click entry.
//!
//! macOS is the only desktop with a supported way for one app to add an item to the
//! context menu of every other app: a system Service. Windows and Linux draw that menu
//! inside each application, so there the global shortcut and the tray icon are the way in.

use std::sync::mpsc::Receiver;

/// Signals sent when the user picks "Multi Paste" from a context menu.
pub type Trigger = Receiver<()>;

#[cfg(target_os = "macos")]
mod platform {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{AllocAnyThread, MainThreadMarker, define_class, msg_send};
    use objc2_app_kit::{NSApplication, NSUpdateDynamicServices};
    use std::sync::OnceLock;
    use std::sync::mpsc::{self, Sender};

    static TRIGGER: OnceLock<Sender<()>> = OnceLock::new();

    define_class!(
        // SAFETY: NSObject has no subclassing requirements, and this type has no ivars
        // and no Drop implementation.
        #[unsafe(super(NSObject))]
        #[name = "MultiPasteServiceProvider"]
        struct ServiceProvider;

        impl ServiceProvider {
            /// Invoked by macOS when the Service is chosen. The picker cannot open
            /// synchronously here — the UI needs this thread — so this only rings the
            /// bell and returns, leaving the pasteboard untouched.
            #[unsafe(method(multiPaste:userData:error:))]
            fn multi_paste(
                &self,
                _pasteboard: *mut AnyObject,
                _user_data: *mut AnyObject,
                _error: *mut AnyObject,
            ) {
                if let Some(sender) = TRIGGER.get() {
                    let _ = sender.send(());
                }
            }
        }
    );

    pub fn install() -> super::Trigger {
        let (sender, receiver) = mpsc::channel();
        let _ = TRIGGER.set(sender);

        let Some(mtm) = MainThreadMarker::new() else {
            eprintln!("multipaste: services must be installed from the main thread");
            return receiver;
        };

        let provider: Retained<ServiceProvider> =
            unsafe { msg_send![ServiceProvider::alloc(), init] };
        let app = NSApplication::sharedApplication(mtm);
        // SAFETY: the provider outlives the application, see the leak below.
        unsafe { app.setServicesProvider(Some(&provider)) };
        std::mem::forget(provider);

        // Without this the new Service only shows up after a login or a rescan.
        NSUpdateDynamicServices();
        receiver
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    pub fn install() -> super::Trigger {
        // The sender is dropped immediately, so the receiver simply never fires.
        std::sync::mpsc::channel().1
    }
}

pub use platform::install;
