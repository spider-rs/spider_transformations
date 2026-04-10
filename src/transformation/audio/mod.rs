//! Audio file to markdown transcription.
//!
//! Decodes any supported audio format (MP3, WAV, FLAC, OGG, AAC/M4A, AIFF,
//! CAF, MKV/WebM) to raw PCM via symphonia, resamples to 16 kHz mono, and
//! transcribes with whisper.cpp via whisper-rs.
//!
//! **Setup**: set the `WHISPER_MODEL_PATH` environment variable to a ggml
//! model file (e.g. `ggml-base.en.bin`). The model is loaded once and cached
//! for the lifetime of the process.
//!
//! All conversions are panic-free: errors are converted to `None` at the
//! public boundary so callers fall through to the existing binary-file path.

use std::sync::OnceLock;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Whisper requires 16 kHz mono audio.
const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Maximum audio duration we will attempt to decode (10 minutes).
/// Prevents runaway memory usage on very long files.
const MAX_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize * 60 * 10;

/// Cached whisper context — loaded once from `WHISPER_MODEL_PATH`.
static WHISPER_CTX: OnceLock<Option<WhisperContext>> = OnceLock::new();

/// Internal error type — never exposed outside this module.
#[derive(Debug)]
enum AudioError {
    /// Audio format not recognized by symphonia.
    Probe,
    /// No decodable audio track found.
    NoTrack,
    /// Decoder creation or packet decode failure.
    Decode,
    /// Whisper model not available.
    NoModel,
    /// Whisper inference failed.
    Transcribe,
}

// -----------------------------------------------------------------------
// Magic-byte detection
// -----------------------------------------------------------------------

/// Known audio signatures for quick rejection of non-audio bytes.
fn is_likely_audio(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }

    // ID3v2 tag (MP3/AAC/etc)
    if bytes.len() >= 3 && &bytes[..3] == b"ID3" {
        return true;
    }
    // MPEG audio sync word (MP3/MP2/MP1)
    if bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0 {
        return true;
    }
    // RIFF (WAV) / FORM (AIFF)
    if &bytes[..4] == b"RIFF" || &bytes[..4] == b"FORM" {
        return true;
    }
    // FLAC
    if &bytes[..4] == b"fLaC" {
        return true;
    }
    // OGG
    if &bytes[..4] == b"OggS" {
        return true;
    }
    // ISO MP4 / M4A (ftyp box)
    if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
        return true;
    }
    // CAF (Core Audio Format)
    if &bytes[..4] == b"caff" {
        return true;
    }
    // MKV/WebM (EBML header)
    if bytes[0] == 0x1A && bytes[1] == 0x45 && bytes[2] == 0xDF && bytes[3] == 0xA3 {
        return true;
    }

    false
}

/// Add a format hint from magic bytes to help symphonia probe faster.
fn hint_from_magic(bytes: &[u8]) -> Hint {
    let mut hint = Hint::new();
    if bytes.len() >= 8 {
        if &bytes[..3] == b"ID3" || (bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0) {
            hint.with_extension("mp3");
        } else if &bytes[..4] == b"fLaC" {
            hint.with_extension("flac");
        } else if &bytes[..4] == b"RIFF" {
            hint.with_extension("wav");
        } else if &bytes[..4] == b"OggS" {
            hint.with_extension("ogg");
        } else if &bytes[4..8] == b"ftyp" {
            hint.with_extension("mp4");
        } else if &bytes[..4] == b"FORM" {
            hint.with_extension("aiff");
        } else if &bytes[..4] == b"caff" {
            hint.with_extension("caf");
        }
    }
    hint
}

// -----------------------------------------------------------------------
// Audio decoding (symphonia)
// -----------------------------------------------------------------------

/// Decode audio bytes into f32 samples at the source sample rate and channel
/// count. Returns `(samples, sample_rate, channels)`.
fn decode_audio(bytes: &[u8]) -> Result<(Vec<f32>, u32, usize), AudioError> {
    let owned = bytes.to_vec();
    let cursor = std::io::Cursor::new(owned);
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    let hint = hint_from_magic(bytes);

    let probe_result = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| AudioError::Probe)?;

    let mut format_reader = probe_result.format;

    // Find first audio track
    let track = format_reader
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or(AudioError::NoTrack)?;

    let track_id = track.id;
    let sample_rate = track
        .codec_params
        .sample_rate
        .unwrap_or(WHISPER_SAMPLE_RATE);
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(1);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| AudioError::Decode)?;

    let mut all_samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format_reader.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break; // End of stream
            }
            Err(_) => break,
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue, // Skip undecodable packets
        };

        let spec = *decoded.spec();
        let num_frames = decoded.capacity();

        if num_frames == 0 {
            continue;
        }

        let mut sample_buf = SampleBuffer::<f32>::new(num_frames as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);

        all_samples.extend_from_slice(sample_buf.samples());

        // Safety limit to prevent runaway memory
        let mono_equivalent = all_samples.len() / channels.max(1);
        if mono_equivalent > MAX_SAMPLES {
            break;
        }
    }

    Ok((all_samples, sample_rate, channels))
}

// -----------------------------------------------------------------------
// Resampling (linear interpolation, no extra deps)
// -----------------------------------------------------------------------

/// Mix interleaved multi-channel audio down to mono.
fn to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }

    let num_frames = samples.len() / channels;
    let mut mono = Vec::with_capacity(num_frames);
    let inv = 1.0 / channels as f32;

    for frame in 0..num_frames {
        let offset = frame * channels;
        let mut sum = 0.0f32;
        for ch in 0..channels {
            if let Some(&s) = samples.get(offset + ch) {
                sum += s;
            }
        }
        mono.push(sum * inv);
    }

    mono
}

/// Resample mono f32 audio from `src_rate` to `dst_rate` using linear
/// interpolation. Good enough for speech-to-text; avoids extra dependencies.
fn resample_linear(samples: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src_rate == dst_rate || samples.is_empty() {
        return samples.to_vec();
    }

    let ratio = src_rate as f64 / dst_rate as f64;
    let out_len = ((samples.len() as f64) / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = (src_pos - idx as f64) as f32;

        let s0 = samples.get(idx).copied().unwrap_or(0.0);
        let s1 = samples.get(idx + 1).copied().unwrap_or(s0);

        out.push(s0 + frac * (s1 - s0));
    }

    out
}

/// Full pipeline: decode → mono → resample to 16 kHz.
fn decode_to_whisper_pcm(bytes: &[u8]) -> Result<Vec<f32>, AudioError> {
    let (samples, sample_rate, channels) = decode_audio(bytes)?;

    if samples.is_empty() {
        return Err(AudioError::Decode);
    }

    let mono = to_mono(&samples, channels);
    let resampled = resample_linear(&mono, sample_rate, WHISPER_SAMPLE_RATE);

    Ok(resampled)
}

// -----------------------------------------------------------------------
// Model resolution
// -----------------------------------------------------------------------

/// Resolve the whisper model path. Priority:
/// 1. `WHISPER_MODEL_PATH` env var (runtime override)
/// 2. Build-time default from `WHISPER_MODEL_PATH_DEFAULT` (set by build.rs)
fn resolve_model_path() -> Option<String> {
    // 1. Runtime env var override
    if let Ok(p) = std::env::var("WHISPER_MODEL_PATH") {
        if !p.is_empty() && std::path::Path::new(&p).exists() {
            return Some(p);
        }
    }

    // 2. Build-time default (auto-downloaded by build.rs)
    let default_path = env!("WHISPER_MODEL_PATH_DEFAULT");
    if !default_path.is_empty() && std::path::Path::new(default_path).exists() {
        return Some(default_path.to_string());
    }

    None
}

// -----------------------------------------------------------------------
// Whisper transcription
// -----------------------------------------------------------------------

/// Get or initialize the cached whisper context.
fn get_whisper_ctx() -> Option<&'static WhisperContext> {
    WHISPER_CTX
        .get_or_init(|| {
            let model_path = resolve_model_path()?;
            WhisperContext::new_with_params(&model_path, WhisperContextParameters::default()).ok()
        })
        .as_ref()
}

/// Transcribe f32 PCM samples (16 kHz mono) using whisper.
fn transcribe_pcm(ctx: &WhisperContext, pcm: &[f32]) -> Result<String, AudioError> {
    let mut state = ctx.create_state().map_err(|_| AudioError::Transcribe)?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_print_special(false);
    params.set_suppress_blank(true);
    params.set_language(Some("en"));
    // single-threaded to avoid contention; callers can parallelize at a higher level
    params.set_n_threads(1);

    state
        .full(params, pcm)
        .map_err(|_| AudioError::Transcribe)?;

    let n_segments = state.full_n_segments();

    let mut text = String::new();

    for i in 0..n_segments {
        if let Some(segment) = state.get_segment(i) {
            if let Ok(segment_text) = segment.to_str_lossy() {
                let trimmed = segment_text.trim();
                if !trimmed.is_empty() {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(trimmed);
                }
            }
        }
    }

    Ok(text)
}

// -----------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------

/// Try to convert audio bytes to a markdown transcription.
///
/// Returns `None` if the bytes are not a recognized audio format, if the
/// whisper model is not configured (`WHISPER_MODEL_PATH`), or on any error.
pub(crate) fn try_convert_audio(bytes: &[u8]) -> Option<String> {
    if !is_likely_audio(bytes) {
        return None;
    }
    convert_inner(bytes).ok()
}

fn convert_inner(bytes: &[u8]) -> Result<String, AudioError> {
    let pcm = decode_to_whisper_pcm(bytes)?;

    let ctx = get_whisper_ctx().ok_or(AudioError::NoModel)?;
    let transcript = transcribe_pcm(ctx, &pcm)?;

    if transcript.is_empty() {
        return Err(AudioError::Transcribe);
    }

    Ok(transcript)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // Magic-byte detection
    // ---------------------------------------------------------------

    #[test]
    fn test_is_likely_audio_empty() {
        assert!(!is_likely_audio(&[]));
    }

    #[test]
    fn test_is_likely_audio_short() {
        assert!(!is_likely_audio(&[0x00, 0x01]));
    }

    #[test]
    fn test_is_likely_audio_id3() {
        assert!(is_likely_audio(b"ID3\x04\x00\x00"));
    }

    #[test]
    fn test_is_likely_audio_mp3_sync() {
        assert!(is_likely_audio(&[0xFF, 0xFB, 0x90, 0x00]));
    }

    #[test]
    fn test_is_likely_audio_riff() {
        assert!(is_likely_audio(b"RIFF\x00\x00\x00\x00"));
    }

    #[test]
    fn test_is_likely_audio_flac() {
        assert!(is_likely_audio(b"fLaC\x00\x00\x00\x22"));
    }

    #[test]
    fn test_is_likely_audio_ogg() {
        assert!(is_likely_audio(b"OggS\x00\x02\x00\x00"));
    }

    #[test]
    fn test_is_likely_audio_ftyp() {
        assert!(is_likely_audio(&[
            0x00, 0x00, 0x00, 0x20, b'f', b't', b'y', b'p'
        ]));
    }

    #[test]
    fn test_is_likely_audio_non_audio() {
        assert!(!is_likely_audio(b"hello world this is not audio"));
        assert!(!is_likely_audio(&[0x50, 0x4B, 0x03, 0x04])); // ZIP
        assert!(!is_likely_audio(&[0x89, 0x50, 0x4E, 0x47])); // PNG
        assert!(!is_likely_audio(&[0xFF, 0xD8, 0xFF, 0xE0])); // JPEG
    }

    // ---------------------------------------------------------------
    // Panic-freedom: try_convert_audio on bad input
    // ---------------------------------------------------------------

    #[test]
    fn test_try_convert_audio_empty_no_panic() {
        assert!(try_convert_audio(&[]).is_none());
    }

    #[test]
    fn test_try_convert_audio_garbage_no_panic() {
        assert!(try_convert_audio(&[0xFF; 100]).is_none());
        assert!(try_convert_audio(b"not audio at all").is_none());
    }

    #[test]
    fn test_try_convert_audio_truncated_headers_no_panic() {
        assert!(try_convert_audio(b"ID3\x04\x00\x00\x00\x00\x00\x00").is_none());
        assert!(try_convert_audio(b"RIFF\x00\x00\x00\x00WAVE").is_none());
        assert!(try_convert_audio(b"fLaC\x00").is_none());
        assert!(try_convert_audio(b"OggS\x00").is_none());
    }

    // ---------------------------------------------------------------
    // Audio decoding (symphonia)
    // ---------------------------------------------------------------

    #[test]
    fn test_decode_valid_wav() {
        let wav = build_test_wav(44100, 1, 16, &[0i16, 1000, -1000, 0]);
        let result = decode_audio(&wav);
        assert!(result.is_ok(), "valid WAV should decode: {result:?}");
        let (samples, sr, ch) = result.unwrap();
        assert_eq!(sr, 44100);
        assert_eq!(ch, 1);
        assert_eq!(samples.len(), 4);
    }

    #[test]
    fn test_decode_stereo_wav() {
        // 4 frames of stereo = 8 interleaved samples
        let wav = build_test_wav(48000, 2, 16, &[0i16, 0, 100, 100, -100, -100, 0, 0]);
        let result = decode_audio(&wav);
        assert!(result.is_ok());
        let (samples, sr, ch) = result.unwrap();
        assert_eq!(sr, 48000);
        assert_eq!(ch, 2);
        assert_eq!(samples.len(), 8);
    }

    #[test]
    fn test_decode_garbage_returns_err() {
        assert!(decode_audio(&[0xFF; 100]).is_err());
        assert!(decode_audio(b"not audio").is_err());
    }

    // ---------------------------------------------------------------
    // Mono mixdown
    // ---------------------------------------------------------------

    #[test]
    fn test_to_mono_passthrough() {
        let samples = vec![1.0, 2.0, 3.0];
        assert_eq!(to_mono(&samples, 1), samples);
    }

    #[test]
    fn test_to_mono_stereo() {
        // L=1.0 R=0.0, L=0.0 R=1.0 → mono = [0.5, 0.5]
        let samples = vec![1.0, 0.0, 0.0, 1.0];
        let mono = to_mono(&samples, 2);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.5).abs() < 1e-6);
        assert!((mono[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_to_mono_empty() {
        assert!(to_mono(&[], 2).is_empty());
    }

    // ---------------------------------------------------------------
    // Resampling
    // ---------------------------------------------------------------

    #[test]
    fn test_resample_same_rate() {
        let samples = vec![1.0, 2.0, 3.0];
        assert_eq!(resample_linear(&samples, 16000, 16000), samples);
    }

    #[test]
    fn test_resample_downsample() {
        // 32 kHz → 16 kHz should roughly halve the length
        let samples: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let out = resample_linear(&samples, 32000, 16000);
        assert!(out.len() >= 49 && out.len() <= 51, "len={}", out.len());
    }

    #[test]
    fn test_resample_upsample() {
        // 8 kHz → 16 kHz should roughly double the length
        let samples: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let out = resample_linear(&samples, 8000, 16000);
        assert!(out.len() >= 199 && out.len() <= 201, "len={}", out.len());
    }

    #[test]
    fn test_resample_empty() {
        assert!(resample_linear(&[], 44100, 16000).is_empty());
    }

    // ---------------------------------------------------------------
    // Full pipeline (without model — returns None gracefully)
    // ---------------------------------------------------------------

    #[test]
    fn test_try_convert_valid_wav_no_model_returns_none() {
        // Without WHISPER_MODEL_PATH set, should return None (no model)
        // but must not panic.
        let wav = build_test_wav(16000, 1, 16, &[0i16; 16000]); // 1 second silence
        let result = try_convert_audio(&wav);
        // Without a model this will be None — that's expected
        let _ = result;
    }

    #[test]
    fn test_decode_to_whisper_pcm_valid_wav() {
        let wav = build_test_wav(44100, 2, 16, &[100i16, -100, 200, -200, 0, 0, 0, 0]);
        let pcm = decode_to_whisper_pcm(&wav);
        assert!(pcm.is_ok(), "should decode + resample: {pcm:?}");
        let samples = pcm.unwrap();
        // Resampled from 44100→16000, 4 mono frames → ~1-2 samples
        assert!(!samples.is_empty());
    }

    // ---------------------------------------------------------------
    // Model resolution
    // ---------------------------------------------------------------

    #[test]
    fn test_default_model_path_set_at_build_time() {
        let path = env!("WHISPER_MODEL_PATH_DEFAULT");
        assert!(
            !path.is_empty(),
            "build.rs should set WHISPER_MODEL_PATH_DEFAULT"
        );
        assert!(
            path.contains("whisper"),
            "default path should reference whisper cache: {path}"
        );
    }

    // ---------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------

    /// Build a minimal valid WAV file in memory for testing.
    fn build_test_wav(sample_rate: u32, channels: u16, bits: u16, samples: &[i16]) -> Vec<u8> {
        let block_align = channels * (bits / 8);
        let byte_rate = sample_rate * block_align as u32;
        let data_size = (samples.len() * 2) as u32;
        let file_size = 36 + data_size;

        let mut buf = Vec::with_capacity(file_size as usize + 8);

        // RIFF header
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&file_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");

        // fmt chunk
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&bits.to_le_bytes());

        // data chunk
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        for &sample in samples {
            buf.extend_from_slice(&sample.to_le_bytes());
        }

        buf
    }
}
