use crate::{errors::analyzer::AnalyzerError, models::audio::AudioInfo};

pub fn wave_analyzer() -> Result<AudioInfo, AnalyzerError> {
    // read wav file
    let reader = hound::WavReader::open("data/s_s002.wav")?;

    // get headers
    let spec = reader.spec();
    tracing::info!(
        "WAV: {}Hz, {}ch, {}bits",
        spec.sample_rate,
        spec.channels,
        spec.bits_per_sample
    );

    Ok(AudioInfo {
        channel: (spec.channels as u32),
        sample_rate: spec.sample_rate,
    })
}
