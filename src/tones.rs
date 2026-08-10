use crate::platform;
use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;

#[derive(Clone, Copy, Debug)]
pub struct Tone {
    pub freq_hz: u32,
    pub duration_ms: u32,
}

const fn t(freq_hz: u32, duration_ms: u32) -> Tone {
    Tone {
        freq_hz,
        duration_ms,
    }
}

pub const TONE_ERROR: &[Tone] = &[t(220, 120)];
pub const TONE_WARNING: &[Tone] = &[t(440, 90)];
/// Zwei aufsteigende Töne bei neuer Ausgabe.
pub const TONE_ACTIVITY: &[Tone] = &[t(880, 60), t(1175, 60)];
pub const TONE_LAYER_ON: &[Tone] = &[t(1319, 50)];
pub const TONE_LAYER_OFF: &[Tone] = &[t(659, 50)];
pub const TONE_BOOKMARK_SET: &[Tone] = &[t(1047, 60)];
pub const TONE_BOOKMARK_REMOVED: &[Tone] = &[t(523, 60)];
pub const TONE_NO_BOOKMARKS: &[Tone] = &[t(330, 80)];

/// Plays tone sequences on a dedicated thread, since the underlying
/// platform beep blocks for the tone's duration and must never stall the
/// render loop or the accessibility message pump.
pub struct Tones {
    tx: Option<Sender<&'static [Tone]>>,
    thread: Option<JoinHandle<()>>,
}

impl Tones {
    pub fn new() -> Self {
        let (tx, rx) = channel::<&'static [Tone]>();
        let thread = std::thread::spawn(move || {
            while let Ok(sequence) = rx.recv() {
                for tone in sequence {
                    platform::beep(tone.freq_hz, tone.duration_ms);
                }
            }
        });
        Self {
            tx: Some(tx),
            thread: Some(thread),
        }
    }

    pub fn play(&self, sequence: &'static [Tone]) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(sequence);
        }
    }
}

impl Default for Tones {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Tones {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
