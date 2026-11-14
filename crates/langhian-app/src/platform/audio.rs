use langhian_core::SpeechAudio;
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink};
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

enum AudioCommand {
    Play(SpeechAudio, mpsc::SyncSender<Result<(), String>>),
    Stop,
}

/// Non-blocking cross-platform desktop audio playback controller backed by `rodio`.
#[derive(Clone)]
pub struct AudioPlayer {
    tx: Sender<AudioCommand>,
    is_playing: Arc<AtomicBool>,
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPlayer {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let is_playing = Arc::new(AtomicBool::new(false));
        let playing_flag = Arc::clone(&is_playing);

        std::thread::Builder::new()
            .name("langhian-audio".to_string())
            .spawn(move || audio_worker(rx, playing_flag))
            .ok();

        Self { tx, is_playing }
    }

    pub fn play(&self, audio: &SpeechAudio) -> Result<(), String> {
        if audio.bytes.is_empty() {
            return Err("Audio payload is empty".to_string());
        }
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        self.tx
            .send(AudioCommand::Play(audio.clone(), ack_tx))
            .map_err(|e| format!("Audio worker unavailable: {e}"))?;
        ack_rx
            .recv_timeout(Duration::from_secs(3))
            .map_err(|e| format!("Audio worker timed out: {e}"))?
    }

    pub fn stop(&self) {
        self.is_playing.store(false, Ordering::Relaxed);
        let _ = self.tx.send(AudioCommand::Stop);
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed)
    }
}

struct ActivePlayback {
    _stream: OutputStream,
    sink: Sink,
}

fn audio_worker(rx: Receiver<AudioCommand>, is_playing: Arc<AtomicBool>) {
    let mut active: Option<ActivePlayback> = None;

    loop {
        let cmd = if active.is_some() {
            match rx.recv_timeout(Duration::from_millis(80)) {
                Ok(cmd) => Some(cmd),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(pb) = &active
                        && pb.sink.empty()
                    {
                        active = None;
                        is_playing.store(false, Ordering::Relaxed);
                    }
                    None
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(cmd) => Some(cmd),
                Err(_) => break,
            }
        };

        let Some(cmd) = cmd else {
            continue;
        };

        match cmd {
            AudioCommand::Stop => {
                if let Some(pb) = active.take() {
                    pb.sink.stop();
                }
                is_playing.store(false, Ordering::Relaxed);
            }
            AudioCommand::Play(audio, ack_tx) => {
                if let Some(pb) = active.take() {
                    pb.sink.stop();
                }

                match start_playback(audio) {
                    Ok(pb) => {
                        is_playing.store(true, Ordering::Relaxed);
                        active = Some(pb);
                        let _ = ack_tx.send(Ok(()));
                    }
                    Err(err) => {
                        is_playing.store(false, Ordering::Relaxed);
                        let _ = ack_tx.send(Err(err));
                    }
                }
            }
        }
    }
}

fn start_playback(audio: SpeechAudio) -> Result<ActivePlayback, String> {
    if audio.bytes.is_empty() {
        return Err("Audio payload is empty".to_string());
    }

    let stream = OutputStreamBuilder::open_default_stream()
        .map_err(|e| format!("Failed to open audio output stream: {e}"))?;
    let sink = Sink::connect_new(stream.mixer());
    let source = Decoder::new(Cursor::new(audio.bytes))
        .map_err(|e| format!("Failed to decode generated audio: {e}"))?;
    sink.append(source);
    sink.play();

    Ok(ActivePlayback {
        _stream: stream,
        sink,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use langhian_core::{AudioFormat, pcm16_le_to_wav};

    #[test]
    fn decodes_pcm_wav_and_rejects_empty_payload() {
        let player = AudioPlayer::new();
        let empty = SpeechAudio {
            bytes: Vec::new(),
            format: AudioFormat::Wav,
        };
        assert!(player.play(&empty).is_err());

        // 0.1s mono 24kHz silent PCM16 wrapped as WAV should decode cleanly via rodio/hound
        let pcm = vec![0_u8; 4800];
        let wav = pcm16_le_to_wav(&pcm, 24_000, 1);
        let decoded = Decoder::new(Cursor::new(wav));
        assert!(decoded.is_ok());
    }
}
