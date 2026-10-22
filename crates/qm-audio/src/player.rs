//! WASAPI playback of a [`PlayShared`] buffer to an output device (the default speakers/headphones, never the
//! virtual cable), used by the in-app test recorder.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Result};
use wasapi::{initialize_mta, DeviceEnumerator, Direction, StreamMode};

use crate::player_core::PlayShared;
use crate::session::{mono_f32_format, pro_audio_priority};

pub struct Player {
    stop: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    handle: Option<JoinHandle<()>>,
}

impl Player {
    /// Open the output device and start pulling audio from `shared` (silent until `shared.play()`).
    pub fn start(shared: Arc<PlayShared>, output_id: Option<String>) -> Result<Player> {
        let stop = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let (s2, e2) = (stop.clone(), error.clone());
        let handle = thread::Builder::new().name("qm-player".into()).spawn(move || {
            if let Err(e) = play_main(&shared, &s2, output_id) {
                *e2.lock().unwrap() = Some(format!("{e:#}"));
            }
        })?;
        Ok(Player { stop, error, handle: Some(handle) })
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn play_main(shared: &PlayShared, stop: &AtomicBool, output_id: Option<String>) -> Result<()> {
    let _ = initialize_mta();
    pro_audio_priority();
    let en = DeviceEnumerator::new()?;
    let dev = match output_id {
        Some(id) => en.get_device(&id)?,
        None => en.get_default_device(&Direction::Render)?,
    };
    let mut client = dev.get_iaudioclient()?;
    let (period, _) = client.get_device_period()?;
    client.initialize_client(&mono_f32_format(), &Direction::Render, &StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: period })?;
    let event = client.set_get_eventhandle()?;
    let render = client.get_audiorenderclient()?;
    let space = client.get_available_space_in_frames()? as usize;
    render.write_to_device(space, &vec![0u8; space * 4], None)?;
    client.start_stream()?;

    let mut block: Vec<f32> = Vec::new();
    let mut bytes: Vec<u8> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        if event.wait_for_event(1000).is_err() {
            return Err(anyhow!("the output device stopped asking for audio"));
        }
        let frames = client.get_available_space_in_frames()? as usize;
        if frames == 0 {
            continue;
        }
        block.clear();
        block.resize(frames, 0.0);
        shared.fill(&mut block);
        bytes.clear();
        bytes.extend(block.iter().flat_map(|s| s.to_le_bytes()));
        render.write_to_device(frames, &bytes, None)?;
    }
    // let the last block play out so the end of the recording is not cut off
    thread::sleep(Duration::from_millis(60));
    client.stop_stream()?;
    Ok(())
}
