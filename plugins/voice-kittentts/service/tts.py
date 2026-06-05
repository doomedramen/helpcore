"""Text-to-speech via KittenTTS — lazy-loaded singleton."""

import io
import threading

import numpy as np
import soundfile as sf

from config import settings

_lock = threading.Lock()
_model = None


def _get_model():
    """Return the KittenTTS model, loading it on first call."""
    global _model
    if _model is None:
        with _lock:
            if _model is None:
                # Import here so startup is fast even if the model load is slow
                from kittentts import KittenTTS  # type: ignore[import]
                _model = KittenTTS(settings.kitten_model)
    return _model


def synthesize(text: str, voice: str | None = None) -> bytes:
    """Convert text to WAV audio bytes.

    Args:
        text:  Text to speak.
        voice: Voice name; falls back to KITTEN_VOICE env var default.

    Returns:
        WAV-encoded audio as bytes (24 kHz, mono float32).
    """
    chosen_voice = voice or settings.kitten_voice
    model = _get_model()

    # KittenTTS returns a numpy array sampled at 24 kHz
    audio_array: np.ndarray = model.generate(text, voice=chosen_voice)

    buf = io.BytesIO()
    sf.write(buf, audio_array, samplerate=24000, format="WAV", subtype="FLOAT")
    buf.seek(0)
    return buf.read()
