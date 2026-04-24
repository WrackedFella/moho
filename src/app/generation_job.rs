//! World generation job — owns the async generation channel, thread handle,
//! cancellation flag, and the most-recently-used WorldSpec.

use crossbeam_channel::Receiver;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct WorldGenerationJob {
    pub receiver: Option<Receiver<crate::GenerationMsg>>,
    pub handle: Option<std::thread::JoinHandle<()>>,
    pub cancel: Option<Arc<AtomicBool>>,
    pub last_spec: Option<moho_core::scene_builders::WorldSpec>,
}

impl WorldGenerationJob {
    pub fn new() -> Self {
        Self {
            receiver: None,
            handle: None,
            cancel: None,
            last_spec: None,
        }
    }

    /// Signal cancellation and block until the generation thread exits.
    pub fn cancel_and_wait(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::SeqCst);
        }
        if let Some(handle) = self.handle.take() {
            log::info!("Waiting for background generation to finish...");
            if handle.join().is_err() {
                log::error!("Failed to join generation thread");
            }
        }
        self.cancel = None;
        self.receiver = None;
    }
}

impl Default for WorldGenerationJob {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WorldGenerationJob {
    fn drop(&mut self) {
        self.cancel_and_wait();
    }
}
