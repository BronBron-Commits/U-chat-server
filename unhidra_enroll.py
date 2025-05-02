import os
import time
import numpy as np
import soundfile as sf
from speechbrain.inference.speaker import SpeakerRecognition
import sounddevice as sd
import webrtcvad
import collections
import matplotlib.pyplot as plt

# === Terminal formatting ===
RED = '\033[91m'
GREEN = '\033[92m'
CYAN = '\033[96m'
YELLOW = '\033[93m'
RESET = '\033[0m'

# === Robust sample rate detection ===
def find_working_samplerate():
    for rate in [48000, 44100, 32000, 16000]:
        try:
            with sd.InputStream(samplerate=rate, channels=1):
                return rate
        except Exception:
            continue
    return 16000

samplerate = find_working_samplerate()
print(f"{CYAN}🎚 Using input sample rate: {samplerate}{RESET}")

# === Device Selection ===
def list_input_devices():
    """
    Lists all available input devices and lets the user select one.
    """
    print("Available input devices:")
    devices = sd.query_devices()
    input_devices = []
    for idx, device in enumerate(devices):
        if device['max_input_channels'] > 0:
            print(f"{idx}: {device['name']}")
            input_devices.append(device)

    # Prompt user to select a device
    while True:
        try:
            device_number = int(input(f"Select the microphone device number from the list above: "))
            if 0 <= device_number < len(input_devices):
                selected_device = input_devices[device_number]
                print(f"Selected device: {selected_device['name']}")
                return selected_device['name'], device_number  # return both name and ID
            else:
                print(f"Invalid selection. Please select a valid device number.")
        except ValueError:
            print("Invalid input. Please enter a valid device number.")

# List devices and let the user select
selected_device_name, selected_device = list_input_devices()

# === Voice-activity-based recording ===
def record_until_silence(sample_rate=48000, frame_duration_ms=30, max_record_sec=10, silence_sec=1.5):
    vad = webrtcvad.Vad(2)
    frame_size = int(sample_rate * frame_duration_ms / 1000)
    num_padding_frames = int(silence_sec * 1000 / frame_duration_ms)
    ring_buffer = collections.deque(maxlen=num_padding_frames)
    recording = []
    triggered = False

    # Open the stream with the selected device
    stream = sd.InputStream(samplerate=sample_rate, channels=1, dtype="int16", blocksize=frame_size, device=selected_device)

    # Setup plot for audio level feedback
    plt.ion()  # Turn interactive mode on
    fig, ax = plt.subplots(figsize=(10, 2))
    x = np.arange(0, 100, 1)
    y = np.zeros(100)
    line, = ax.plot(x, y, color='green')
    ax.set_ylim(0, 255)
    ax.set_title('Microphone Input Level')
    ax.set_xlabel('Time')
    ax.set_ylabel('Volume')

    with stream:
        print(f"{CYAN}   🎤 Speak now (recording stops after {silence_sec}s of silence)...{RESET}")
        while True:
            audio_chunk, _ = stream.read(frame_size)
            raw = audio_chunk.flatten().tobytes()
            if len(raw) < 640:
                continue
            is_speech = vad.is_speech(raw, sample_rate)

            # Calculate volume level for visual feedback
            volume_norm = np.linalg.norm(np.frombuffer(raw, dtype='int16')) / frame_size
            y = np.roll(y, -1)
            y[-1] = volume_norm * 255  # Scale to 0-255 for display

            # Update plot
            line.set_ydata(y)
            fig.canvas.draw()
            fig.canvas.flush_events()

            # If speech detected
            if not triggered:
                ring_buffer.append((raw, is_speech))
                if sum(1 for _, speech in ring_buffer if speech) > 0.9 * num_padding_frames:
                    triggered = True
                    for r, _ in ring_buffer:
                        recording.append(np.frombuffer(r, dtype="int16"))
                    ring_buffer.clear()
            else:
                recording.append(np.frombuffer(raw, dtype="int16"))
                ring_buffer.append((raw, is_speech))
                if sum(1 for _, speech in ring_buffer if not speech) > 0.9 * num_padding_frames:
                    break
            if len(recording) * frame_duration_ms > max_record_sec * 1000:
                print(f"{YELLOW}   ⏱ Max recording duration reached.{RESET}")
                break

    plt.ioff()  # Turn interactive mode off
    plt.show()  # Keep the plot open after recording ends
    return np.concatenate(recording)

# === Script ===
print(f"{CYAN}╔══════════════════════════════════════════════════╗")
print(f"║               UNHIDRA VOICE ENROLL              ║")
print(f"╚══════════════════════════════════════════════════╝{RESET}\n")

print(f"{GREEN}🔐 Welcome, Initiate. Let's give Unhidra your voice.{RESET}\n")

name = input(f"{CYAN}🖋  Enter a name for this voice profile (e.g. 'bronson'): {RESET}").strip().lower()
if not name:
    print(f"{RED}❌ Invalid name. Aborting.{RESET}")
    exit(1)

print(f"\n{CYAN}🎙 You'll speak 7 training phrases. Each will be recorded and analyzed.{RESET}\n")

phrases = [
    "Okay, let's get to it. Unhidra CLI — open it up. Time to get some work done.",
    "Execute primary command set. Authorization: granted.",
    "Voice link established. Standing by for task list.",
    "Activate terminal mode. Begin session logging now.",
    "I am the only one authorized to issue system orders.",
    "Begin operation. Time to see what Unhidra can really do.",
    "Full override. Engage voiceprint protocol — this is me."
]

model = SpeakerRecognition.from_hparams(source="speechbrain/spkrec-ecapa-voxceleb", savedir="~/.cache/speechbrain/speaker-id")

embeddings = []

for idx, phrase in enumerate(phrases):
    print(f"{CYAN}📜 Line {idx+1}/7:\n“{phrase}”{RESET}")
    input(f"   Press {YELLOW}Enter{RESET} to begin...")

    print(f"{YELLOW}   🎬 Recording in: 3... 2... 1...{RESET}")
    time.sleep(1)

    raw_audio = record_until_silence(sample_rate=samplerate)
    audio = raw_audio.astype("float32") / 32768.0
    filename = f"{name}_voice_{idx+1}.wav"
    sf.write(filename, audio, samplerate)
    print(f"{GREEN}   ✅ Saved: {filename}{RESET}\n")

    signal = model.load_audio(filename)
    signal = model.audio_normalizer(signal, samplerate)
    emb = model.encode_batch(signal).squeeze().detach().numpy()
    embeddings.append(emb)

avg_embedding = np.mean(embeddings, axis=0)
np.save(f"{name}_voiceprint.npy", avg_embedding)

print(f"\n{GREEN}🎉 Voice enrollment complete! Your profile '{name}_voiceprint.npy' is ready.{RESET}")
