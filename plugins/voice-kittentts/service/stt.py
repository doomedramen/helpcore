"""Speech-to-text via whisper.cpp subprocess."""

import asyncio
import os
import subprocess
import tempfile

from config import settings


async def transcribe(audio_bytes: bytes, mime_type: str) -> str:
    """Transcribe audio bytes to text.

    Accepts any audio format; non-WAV is converted via ffmpeg first.
    Returns the stripped transcript string.
    Raises RuntimeError on subprocess failure.
    """
    loop = asyncio.get_running_loop()
    return await loop.run_in_executor(None, _transcribe_sync, audio_bytes, mime_type)


def _transcribe_sync(audio_bytes: bytes, mime_type: str) -> str:
    wav_path: str | None = None
    tmp_input: str | None = None

    try:
        # Write raw audio to a temp file
        suffix = _suffix_for_mime(mime_type)
        with tempfile.NamedTemporaryFile(suffix=suffix, delete=False) as f:
            f.write(audio_bytes)
            tmp_input = f.name

        if suffix != ".wav":
            # Convert to WAV via ffmpeg
            wav_path = tmp_input + ".wav"
            result = subprocess.run(
                ["ffmpeg", "-y", "-i", tmp_input, "-ar", "16000", "-ac", "1", wav_path],
                capture_output=True,
            )
            if result.returncode != 0:
                raise RuntimeError(
                    f"ffmpeg conversion failed:\n{result.stderr.decode(errors='replace')}"
                )
        else:
            wav_path = tmp_input
            tmp_input = None  # same file — don't double-delete

        # Run whisper.cpp
        result = subprocess.run(
            [
                settings.whisper_bin,
                "-m", settings.whisper_model,
                "-f", wav_path,
                "--output-txt",
                "-np",          # no progress output
                "--language", "en",
            ],
            capture_output=True,
        )
        if result.returncode != 0:
            raise RuntimeError(
                f"whisper failed:\n{result.stderr.decode(errors='replace')}"
            )

        # whisper.cpp writes {input}.txt alongside the input file
        txt_path = wav_path + ".txt"
        if not os.path.exists(txt_path):
            raise RuntimeError(f"whisper did not produce expected output file: {txt_path}")

        with open(txt_path, encoding="utf-8") as f:
            transcript = f.read().strip()

        return transcript

    finally:
        for path in [tmp_input, wav_path, (wav_path + ".txt") if wav_path else None]:
            if path and os.path.exists(path):
                try:
                    os.unlink(path)
                except OSError:
                    pass


def _suffix_for_mime(mime_type: str) -> str:
    """Return a file extension for a given MIME type."""
    mapping = {
        "audio/wav": ".wav",
        "audio/wave": ".wav",
        "audio/x-wav": ".wav",
        "audio/mpeg": ".mp3",
        "audio/mp4": ".m4a",
        "audio/ogg": ".ogg",
        "audio/webm": ".webm",
        "audio/flac": ".flac",
    }
    return mapping.get(mime_type.lower().split(";")[0].strip(), ".audio")
