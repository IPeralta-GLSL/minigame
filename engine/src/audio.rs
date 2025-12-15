use kira::{
    AudioManager, AudioManagerSettings, DefaultBackend,
    sound::static_sound::StaticSoundData,
};
use std::io::Cursor;

pub struct AudioEngine {
    manager: AudioManager<DefaultBackend>,
}

impl AudioEngine {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())?;
        Ok(Self { manager })
    }

    pub fn create_sound(&self, bytes: &[u8]) -> Result<StaticSoundData, Box<dyn std::error::Error>> {
        let data = StaticSoundData::from_cursor(Cursor::new(bytes.to_vec()))?;
        Ok(data)
    }

    pub fn play(&mut self, sound: &StaticSoundData) {
        let _ = self.manager.play(sound.clone());
    }
}
