use numpy::IntoPyArray;
use pyo3::{
    PyResult, Python,
    types::{PyAnyMethods, PyDict, PyDictMethods},
};

struct WavData {
    samples: Vec<f32>,
    sample_rate: f64,
}

fn read_wav_data(path: &str) -> Result<WavData, hound::Error> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();

    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| s.unwrap() as f32 / i16::MAX as f32)
        .collect();

    let mono_samples = if spec.channels == 2 {
        samples
            .chunks(2)
            .map(|chunk| (chunk[0] + chunk[1]) / 2.0)
            .collect()
    } else {
        samples
    };

    Ok(WavData {
        samples: mono_samples,
        sample_rate: spec.sample_rate as f64,
    })
}

fn example<'py>(py: Python<'py>, wav_data: WavData) -> PyResult<f64> {
    let librosa = py.import("librosa")?;

    let kwargs = PyDict::new(py);
    kwargs.set_item("y", wav_data.samples.into_pyarray(py))?;
    kwargs.set_item("sr", wav_data.sample_rate)?;

    let (tempo, _beats) = librosa
        .getattr("beat")?
        .getattr("beat_track")?
        .call((), Some(&kwargs))?
        .extract::<(f64, pyo3::PyObject)>()?;

    Ok(tempo)
}

fn main() {
    let wav_path = "./data/sample3.wav"; // Replace with your WAV file path

    let wav_data = read_wav_data(wav_path).unwrap();
    let tempo = Python::with_gil(|py| example(py, wav_data)).unwrap();
    println!("Estimated tempo: {tempo} BPM");
}
