use crate::{errors::analyzer::AnalyzerError, models::audio::AudioInfo};

pub fn wave_analyzer() -> Result<AudioInfo, AnalyzerError> {
    // read wav file
    let reader = hound::WavReader::open("data/sample3.wav")?;

    // get headers
    let spec = reader.spec();
    tracing::info!(
        "WAV: {}Hz, {}, {}bits",
        spec.sample_rate,
        if spec.channels == 1 {
            "monoral"
        } else {
            "stereo"
        },
        spec.bits_per_sample
    );

    // calculate chunk size
    let chunk_size = 1024 * spec.channels;

    Ok(AudioInfo {
        channel: spec.channels,
        chunk_size,
    })
}
