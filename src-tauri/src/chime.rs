//! The "new game added" sound. Synthesized here, so there's no audio file from anywhere else.
//! Three sounds: "soft" (a modern two-note chime, the default), "coin" (an 8-bit pickup) and "pop".

use std::f32::consts::TAU;

const RATE: u32 = 44_100;

/// One sound's samples (before volume), `len` seconds long.
fn render(len: f32, f: impl Fn(f32) -> f32) -> Vec<f32> {
    (0..(RATE as f32 * len) as usize).map(|i| f(i as f32 / RATE as f32)).collect()
}

/// Soft attack, then an exponential fade at `decay` per second.
fn env(dt: f32, attack: f32, decay: f32) -> f32 {
    if dt < 0.0 { 0.0 } else if dt < attack { dt / attack } else { (-(dt - attack) * decay).exp() }
}

fn samples(sound: &str) -> Vec<f32> {
    match sound {
        // Square-wave B5 then a held E6, like an old console's coin pickup.
        "coin" => render(1.0, |t| {
            let square = |f: f32, dt: f32| if (f * dt).fract() < 0.5 { 1.0 } else { -1.0 };
            if t < 0.07 { 0.5 * square(987.77, t) } else { 0.5 * env(t - 0.07, 0.002, 5.0) * square(1318.51, t - 0.07) }
        }),
        // A short drop in pitch (a bubble-like pop) with a quick fade.
        "pop" => render(1.0, |t| {
            // Phase of a sine sweeping 1100 -> 330 Hz exponentially over about 50 ms.
            let k = 40.0;
            let phase = 330.0 * t + (1100.0 - 330.0) * (1.0 - (-k * t).exp()) / k;
            env(t, 0.002, 22.0) * (TAU * phase).sin()
        }),
        // A5 then E6, pure tones with a slightly detuned copy for warmth.
        _ => render(1.4, |t| {
            [(0.0, 880.0), (0.14, 1318.51)]
                .iter()
                .map(|(start, f)| {
                    let dt = t - start;
                    env(dt, 0.012, 3.2) * ((TAU * f * dt).sin() + 0.4 * (TAU * (f + 1.5) * dt).sin())
                })
                .sum()
        }),
    }
}

/// The sound as a 16-bit mono WAV file, at `volume` (0 to 100).
pub fn wav(volume: u8, sound: &str) -> Vec<u8> {
    let mut samples = samples(sound);
    let n = samples.len();
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
pub fn play(volume: u8, sound: &str) {
    use std::sync::Mutex;
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    // The sound must stay in memory while Windows plays it.
    static SOUND: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    let mut buffer = SOUND.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: stopping first (null sound) means the old buffer is no longer read before it's replaced;
    // the new buffer lives in the static until the next call stops it again.
    unsafe {
        PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
        *buffer = wav(volume, sound);
        PlaySoundW(buffer.as_ptr() as *const u16, std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

#[cfg(not(windows))]
pub fn play(_: u8, _: &str) {}

#[cfg(test)]
mod tests {
    #[test]
    fn makes_valid_wavs_at_the_right_volume() {
        let peak = |w: &[u8]| w[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs()).max().unwrap();
        for sound in ["soft", "coin", "pop"] {
            let loud = super::wav(100, sound);
            assert_eq!(&loud[..4], b"RIFF");
            assert_eq!(&loud[8..16], b"WAVEfmt ");
            let secs = (loud.len() - 44) as f32 / 2.0 / 44_100.0;
            assert!((1.0..=2.0).contains(&secs), "{sound} lasts 1 to 2 seconds");
            assert!(peak(&loud) > 19_000 && peak(&loud) < 20_000, "{sound}: 60% of full scale");
            assert!(peak(&super::wav(50, sound)) < peak(&loud) / 2 + 2);
            assert_eq!(peak(&super::wav(0, sound)), 0);
        }
        assert_ne!(super::wav(100, "soft"), super::wav(100, "pop"));
        assert_eq!(super::wav(100, "unknown"), super::wav(100, "soft"), "anything else is the default");
    }
}
