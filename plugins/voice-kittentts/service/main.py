"""Voice plugin FastAPI service — STT, TTS, and full voice chat."""

import stt
import tts
import helpcore as helpcore_client

from config import settings
from fastapi import FastAPI, Form, HTTPException, UploadFile
from fastapi.responses import Response
from pydantic import BaseModel

app = FastAPI(title="helpcore voice plugin", version="0.1.0")

VERSION = "0.1.0"


# ── Health ────────────────────────────────────────────────────────────────────

@app.get("/health")
async def health():
    return {"status": "ok", "version": VERSION}


# ── Transcribe ────────────────────────────────────────────────────────────────

@app.post("/transcribe")
async def transcribe(audio: UploadFile):
    """Transcribe an audio file to text.

    Accepts any audio format supported by ffmpeg.
    Returns: {"text": "transcribed text"}
    """
    audio_bytes = await audio.read()
    mime_type = audio.content_type or "audio/wav"
    try:
        text = await stt.transcribe(audio_bytes, mime_type)
    except RuntimeError as e:
        raise HTTPException(status_code=500, detail=str(e))
    return {"text": text}


# ── Synthesize ────────────────────────────────────────────────────────────────

class SynthesizeRequest(BaseModel):
    text: str
    voice: str | None = None


@app.post("/synthesize")
async def synthesize(req: SynthesizeRequest):
    """Convert text to WAV audio.

    Returns raw WAV bytes with Content-Type: audio/wav.
    """
    try:
        wav_bytes = tts.synthesize(req.text, req.voice)
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"TTS error: {e}")
    return Response(content=wav_bytes, media_type="audio/wav")


# ── Voice chat ────────────────────────────────────────────────────────────────

@app.post("/voice/chat")
async def voice_chat(
    audio: UploadFile,
    conversation_id: str | None = Form(default=None),
    provider_id: str | None = Form(default=None),
    model: str | None = Form(default=None),
    voice: str | None = Form(default=None),
):
    """Full voice round-trip: audio → transcript → AI response → audio.

    Request (multipart/form-data):
        audio           — audio file (any ffmpeg-supported format)
        conversation_id — optional, continues an existing conversation
        provider_id     — optional, selects a specific helpcore provider
        model           — optional, selects a specific model
        voice           — optional, overrides KITTEN_VOICE default

    Response:
        audio/wav bytes (the AI's spoken reply)

    Response headers:
        X-Transcript       — what the user said
        X-Response-Text    — what the AI replied (text)
        X-Conversation-Id  — conversation ID (new or existing)
    """
    audio_bytes = await audio.read()
    mime_type = audio.content_type or "audio/wav"

    # Step 1: speech → text
    try:
        transcript = await stt.transcribe(audio_bytes, mime_type)
    except RuntimeError as e:
        raise HTTPException(status_code=500, detail=f"STT error: {e}")

    if not transcript:
        raise HTTPException(status_code=422, detail="Could not transcribe audio — no speech detected")

    # Step 2: text → AI response
    try:
        response_text, returned_conv_id = await helpcore_client.chat(
            message=transcript,
            conversation_id=conversation_id,
            provider_id=provider_id,
            model=model,
        )
    except RuntimeError as e:
        raise HTTPException(status_code=502, detail=f"helpcore error: {e}")

    # Step 3: text → speech
    try:
        wav_bytes = tts.synthesize(response_text, voice)
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"TTS error: {e}")

    return Response(
        content=wav_bytes,
        media_type="audio/wav",
        headers={
            "X-Transcript": transcript,
            "X-Response-Text": response_text,
            "X-Conversation-Id": returned_conv_id,
        },
    )
