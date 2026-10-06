//! The sound card (cpal): the mixer on its thread, the queue's other end for the game.
use crate::{
    def::SoundDefs,
    mixer::{Cmd, Mixer},
    synth::Sound,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;

/// Commands the queue holds (a frame's worth many times over; full, a sound is dropped).
const QUEUE: usize = 512;

pub struct Output {
    /// (Dropped, the sound stops.)
    _stream: cpal::Stream,
    tx: rtrb::Producer<Cmd>,
    /// Sample rate (Hz) and the device's name.
    pub rate: u32,
    pub name: String,
    /// Sound ids in the order of their indices.
    pub ids: Vec<String>,
}

impl Output {
    /// Open the default output and start the mixer on it with `defs` compiled for its rate.
    pub fn open(defs: &SoundDefs) -> Result<Output, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no hay salida de sonido")?;
        let name = device.description().map_or_else(|_| "salida".into(), |d| d.name().to_string());
        let config = device.default_output_config().map_err(|e| format!("{name}: {e}"))?;
        if config.sample_format() != cpal::SampleFormat::F32 {
            return Err(format!("{name}: formato {:?} (solo f32)", config.sample_format()));
        }
        let rate = config.sample_rate();
        let channels = usize::from(config.channels());
        let mut sounds = Vec::with_capacity(defs.len());
        for (id, d) in defs {
            sounds.push(Sound::new(d, rate as f32).map_err(|e| format!("sonido '{id}': {e}"))?);
        }
        let sounds: Arc<[Sound]> = sounds.into();
        let (tx, rx) = rtrb::RingBuffer::new(QUEUE);
        let mut mixer = Mixer::new(sounds, rate as f32, rx);
        let stream = device
            .build_output_stream(config.config(), move |data: &mut [f32], _: &cpal::OutputCallbackInfo| mixer.fill(data, channels), |e| eprintln!("sonido: {e}"), None)
            .map_err(|e| format!("{name}: {e}"))?;
        stream.play().map_err(|e| format!("{name}: {e}"))?;
        Ok(Output { _stream: stream, tx, rate, name, ids: defs.keys().cloned().collect() })
    }

    /// Ask the mixer (dropped if its queue is full).
    pub fn send(&mut self, c: Cmd) {
        let _ = self.tx.push(c);
    }
}
