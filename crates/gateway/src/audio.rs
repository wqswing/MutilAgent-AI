//! Audio processing for speech-to-text transcription.
//!
//! This module provides audio transcription capabilities using
//! Whisper API or compatible services.

use std::sync::Arc;

use multi_agent_core::{
    traits::ArtifactStore,
    types::RefId,
    Error, Result,
};

/// Supported audio formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Mp3,
    Mp4,
    Mpeg,
    Mpga,
    M4a,
    Wav,
    Webm,
    Ogg,
}

impl AudioFormat {
    /// Get the MIME type for this format.
    pub fn mime_type(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "audio/mpeg",
            AudioFormat::Mp4 => "audio/mp4",
            AudioFormat::Mpeg => "audio/mpeg",
            AudioFormat::Mpga => "audio/mpeg",
            AudioFormat::M4a => "audio/mp4",
            AudioFormat::Wav => "audio/wav",
            AudioFormat::Webm => "audio/webm",
            AudioFormat::Ogg => "audio/ogg",
        }
    }

    /// Get the file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Mp4 => "mp4",
            AudioFormat::Mpeg => "mpeg",
            AudioFormat::Mpga => "mpga",
            AudioFormat::M4a => "m4a",
            AudioFormat::Wav => "wav",
            AudioFormat::Webm => "webm",
            AudioFormat::Ogg => "ogg",
        }
    }

    /// Detect format from bytes.
    pub fn detect(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }

        // Check magic bytes
        if data.starts_with(b"RIFF") && data.len() > 8 && &data[8..12] == b"WAVE" {
            return Some(AudioFormat::Wav);
        }
        if data.starts_with(b"OggS") {
            return Some(AudioFormat::Ogg);
        }
        if data.starts_with(&[0xFF, 0xFB]) || data.starts_with(&[0xFF, 0xFA]) {
            return Some(AudioFormat::Mp3);
        }
        if data.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
            return Some(AudioFormat::Webm);
        }
        if data.len() > 4 && &data[4..8] == b"ftyp" {
            return Some(AudioFormat::Mp4);
        }

        None
    }
}

/// Audio processor for transcription.
pub struct AudioProcessor {
    /// Artifact store for storing audio files.
    store: Arc<dyn ArtifactStore>,
    /// OpenAI API key for Whisper.
    openai_api_key: Option<String>,
}

impl AudioProcessor {
    /// Create a new audio processor.
    pub fn new(store: Arc<dyn ArtifactStore>) -> Self {
        Self {
            store,
            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
        }
    }

    /// Store audio in the artifact store.
    pub async fn store_audio(&self, audio_data: &[u8]) -> Result<RefId> {
        let format = AudioFormat::detect(audio_data)
            .map(|f| f.extension().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Encode as base64 for storage
        let content = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            audio_data,
        );

        // Save returns the generated RefId
        let ref_id = self.store.save(content.into()).await?;

        tracing::info!(
            ref_id = %ref_id,
            format = %format,
            size = audio_data.len(),
            "Audio stored"
        );

        Ok(ref_id)
    }

    /// Transcribe audio to text using Whisper API.
    pub async fn transcribe(&self, audio_data: &[u8]) -> Result<TranscriptionResult> {
        let _api_key = self.openai_api_key.as_ref().ok_or_else(|| {
            Error::gateway("OPENAI_API_KEY not set for Whisper transcription".to_string())
        })?;

        let format = AudioFormat::detect(audio_data)
            .ok_or_else(|| Error::gateway("Unknown audio format".to_string()))?;

        tracing::info!(
            format = ?format,
            size = audio_data.len(),
            "Transcribing audio with Whisper"
        );

        // In a full implementation, we would:
        // 1. Create a multipart form request
        // 2. Upload to OpenAI's Whisper API
        // 3. Parse the transcription response

        // Mock response for Phase 3
        Ok(TranscriptionResult {
            text: format!(
                "Audio transcription (mock): A {} audio file of {} bytes was provided. \
                Real transcription would use OpenAI Whisper API.",
                format.extension(),
                audio_data.len()
            ),
            duration_seconds: None,
            language: Some("en".to_string()),
        })
    }

    /// Transcribe audio with language hint.
    pub async fn transcribe_with_language(
        &self,
        audio_data: &[u8],
        language: &str,
    ) -> Result<TranscriptionResult> {
        // Language hint would be passed to Whisper API
        tracing::debug!(language = %language, "Transcribing with language hint");
        self.transcribe(audio_data).await
    }
}

/// Result of audio transcription.
#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    /// The transcribed text.
    pub text: String,
    /// Duration of the audio in seconds.
    pub duration_seconds: Option<f64>,
    /// Detected language.
    pub language: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_format_detection() {
        // WAV header
        let wav = b"RIFF\x00\x00\x00\x00WAVEfmt ";
        assert_eq!(AudioFormat::detect(wav), Some(AudioFormat::Wav));

        // Ogg header
        let ogg = b"OggS\x00\x02";
        assert_eq!(AudioFormat::detect(ogg), Some(AudioFormat::Ogg));

        // MP3 header
        let mp3 = &[0xFF, 0xFB, 0x90, 0x00];
        assert_eq!(AudioFormat::detect(mp3), Some(AudioFormat::Mp3));
    }

    #[test]
    fn test_audio_format_mime_type() {
        assert_eq!(AudioFormat::Mp3.mime_type(), "audio/mpeg");
        assert_eq!(AudioFormat::Wav.mime_type(), "audio/wav");
    }
}
