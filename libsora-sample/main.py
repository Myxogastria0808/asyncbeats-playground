# NOTE: Reference site
# https://tepo-bass.com/python_librosa_tempo/

import librosa

sampling_rate = 44100
filename = "./data/sample3.wav"

# load the audio file
# depended numpy
y, sr = librosa.load(path=filename, sr=sampling_rate)
# extract tempo and beat frames
tempo, beat_frames = librosa.beat.beat_track(y=y, sr=sr)

# result
print(f"Tempo: {tempo} BPM")
