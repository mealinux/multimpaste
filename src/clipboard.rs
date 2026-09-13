use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub type History = Arc<Mutex<Vec<String>>>;

/// Clipboard history, fed by a single background thread that owns the OS clipboard.
///
/// The thread must outlive every copy: on X11 the clipboard contents vanish when the
/// owning process drops its handle, so one long-lived handle is the only safe shape.
pub struct Clip {
    history: History,
    limit: Arc<AtomicUsize>,
    tx: Sender<String>,
}

impl Clip {
    pub fn start(limit: usize) -> Self {
        let history: History = Arc::new(Mutex::new(Vec::new()));
        let limit = Arc::new(AtomicUsize::new(limit.max(1)));
        let (tx, rx) = mpsc::channel::<String>();

        let (h, l) = (history.clone(), limit.clone());
        std::thread::spawn(move || {
            let mut clipboard = match arboard::Clipboard::new() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("multipaste: clipboard unavailable: {e}");
                    return;
                }
            };
            let mut last = String::new();

            loop {
                // Our own copies come through the channel so we never echo them back as
                // a "new" entry, and so the clipboard is only touched from this thread.
                while let Ok(text) = rx.try_recv() {
                    if clipboard.set_text(text.clone()).is_ok() {
                        last = text.clone();
                        remember(&h, &l, text);
                    }
                }

                if let Ok(text) = clipboard.get_text() {
                    if !text.trim().is_empty() && text != last {
                        last = text.clone();
                        remember(&h, &l, text);
                    }
                }

                // ponytail: polling beats three per-OS clipboard-change listeners;
                // swap in `clipboard-master` if 300 ms ever feels laggy.
                std::thread::sleep(Duration::from_millis(300));
            }
        });

        Self { history, limit, tx }
    }

    pub fn entries(&self) -> Vec<String> {
        self.history
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Put `text` back on the clipboard and move it to the top of the history.
    pub fn copy(&self, text: String) {
        let _ = self.tx.send(text);
    }

    pub fn set_limit(&self, limit: usize) {
        let limit = limit.max(1);
        self.limit.store(limit, Ordering::Relaxed);
        self.history
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .truncate(limit);
    }

    pub fn clear(&self) {
        self.history
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }
}

/// Newest first, no duplicates, never longer than the configured limit.
fn remember(history: &History, limit: &AtomicUsize, text: String) {
    let mut entries = history.lock().unwrap_or_else(|e| e.into_inner());
    entries.retain(|existing| existing != &text);
    entries.insert(0, text);
    entries.truncate(limit.load(Ordering::Relaxed).max(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history_of(items: &[&str], limit: usize) -> Vec<String> {
        let history: History = Arc::new(Mutex::new(Vec::new()));
        let cap = AtomicUsize::new(limit);
        for item in items {
            remember(&history, &cap, (*item).to_owned());
        }
        Arc::try_unwrap(history).unwrap().into_inner().unwrap()
    }

    #[test]
    fn newest_entry_comes_first() {
        assert_eq!(history_of(&["a", "b"], 10), vec!["b", "a"]);
    }

    #[test]
    fn recopying_moves_an_entry_up_instead_of_duplicating_it() {
        assert_eq!(history_of(&["a", "b", "a"], 10), vec!["a", "b"]);
    }

    #[test]
    fn the_limit_drops_the_oldest_entries() {
        assert_eq!(history_of(&["a", "b", "c"], 2), vec!["c", "b"]);
    }
}
