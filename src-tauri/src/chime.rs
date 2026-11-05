//! The "new game added" chime. Synthesized here, so there's no audio file from anywhere else:
//! a soft three-note bell (C6, E6, G6) about a second long.

const RATE: u32 = 44_100;
const NOTES: [(f32, f32); 3] = [(0.0, 1046.50), (0.11, 1318.51), (0.22, 1567.98)];
const LENGTH: f32 = 1.2;

/// The chime as a 16-bit mono WAV file, at `volume` (0 to 100).
pub fn wav(volume: u8) -> Vec<u8> {
    let n = (RATE as f32 * LENGTH) as usize;
    let mut samples: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            NOTES
                .iter()
                .filter(|(start, _)| t >= *start)
                .map(|(start, f)| {
                    let dt = t - start;
                    // Quick 5 ms attack, then a bell-like fade; a quiet octave adds shimmer.
                    let env = if dt < 0.005 { dt / 0.005 } else { (-(dt - 0.005) * 4.5).exp() };
                    let w = std::f32::consts::TAU * f * dt;
                    env * (w.sin() + 0.25 * (2.0 * w).sin())
                })
                .sum()
        })
        .collect();
    // Fade the last 50 ms so it never ends with a click, then scale to the chosen volume.
    let fade = (RATE as f32 * 0.05) as usize;
    for (k, s) in samples.iter_mut().rev().take(fade).enumerate() {
        *s *= k as f32 / fade as f32;
    }
    let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs())).max(1e-6);
    let gain = 0.6 * (volume.min(100) as f32 / 100.0) / peak;

    let data_len = (n * 2) as u32;
    let mut out = Vec::with_capacity(44 + n * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // bytes per sample
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s * gain * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

/// Plays the chime without waiting. Windows mixes it with other sound, so it's heard over Big Picture too.
#[cfg(windows)]
pub fn play(volume: u8) {
    use std::sync::Mutex;
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    // The sound must stay in memory while Windows plays it.
    static SOUND: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    let mut sound = SOUND.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: stopping first (null sound) means the old buffer is no longer read before it's replaced;
    // the new buffer lives in the static until the next call stops it again.
    unsafe {
        PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
        *sound = wav(volume);
        PlaySoundW(sound.as_ptr() as *const u16, std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

#[cfg(not(windows))]
pub fn play(_: u8) {}

#[cfg(test)]
mod tests {
    #[test]
    fn makes_a_valid_wav_at_the_right_volume() {
        let loud = super::wav(100);
        assert_eq!(&loud[..4], b"RIFF");
        assert_eq!(&loud[8..16], b"WAVEfmt ");
        assert_eq!(loud.len(), 44 + (44_100.0 * 1.2) as usize * 2);
        let peak = |w: &[u8]| w[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs()).max().unwrap();
        assert!(peak(&loud) > 19_000 && peak(&loud) < 20_000, "60% of full scale");
        assert!(peak(&super::wav(50)) < peak(&loud) / 2 + 2);
        assert_eq!(peak(&super::wav(0)), 0);
        // Small: about 100 KB.
        assert!(loud.len() < 110_000);
    }
}
