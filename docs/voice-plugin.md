# Voice plugin (KittenTTS + Whisper)

The voice plugin adds speech-to-text and text-to-speech to helpcore. It runs as a standalone Docker sidecar — a Python FastAPI service that sits in front of helpcore and handles the audio pipeline.

```
Audio in → [Whisper STT] → text → [helpcore /api/chat] → text → [KittenTTS] → Audio out
```

The AI's skill prompt is automatically injected into the context when the plugin is enabled, telling it to respond in spoken-word style (no markdown, natural sentences, concise).

---

## Requirements

| Component | Notes |
|---|---|
| whisper.cpp binary | Build from https://github.com/ggerganov/whisper.cpp or use a pre-built release |
| Whisper model file | `ggml-base.en.bin` recommended (~150 MB); download with `./models/download-ggml-model.sh base.en` |
| Docker | For the service container |
| helpcore server | Running and accessible |

---

## Setup

### 1. Register the plugin in config.toml

On the helpcore server, add this to `config.toml`:

```toml
[[plugins.local]]
id      = "voice-kittentts"
path    = "plugins/voice-kittentts"
enabled = true
```

Restart helpcore:

```bash
make prod-update   # production
# or
make docker-up     # local dev
```

On restart, the plugin is registered for all users and the skill prompt is active.

### 2. Generate a bridge token

The voice service needs a token to call the helpcore `/api/chat` endpoint:

```bash
hc plugin token voice-kittentts
# → hcp_abc123defg456...
```

Save this token — you'll need it in the next step.

### 3. Prepare whisper.cpp

Build whisper.cpp from source (or download a pre-built binary):

```bash
git clone https://github.com/ggerganov/whisper.cpp
cd whisper.cpp
make -j4

# Download the base English model (~150 MB)
./models/download-ggml-model.sh base.en
```

Put the `whisper-cli` binary and `models/` directory somewhere accessible. A good place is `plugins/voice-kittentts/whisper/` in the repo — that path is already in the compose volume mount.

### 4. Start the voice service

From the `plugins/voice-kittentts/` directory:

```bash
# Create a .env file (or set vars directly)
cat > .env << EOF
HELPCORE_URL=http://helpcore:3000
HELPCORE_TOKEN=hcp_abc123defg456...
WHISPER_BIN=/whisper/whisper-cli
WHISPER_MODEL=/whisper/models/ggml-base.en.bin
KITTEN_VOICE=Jasper
EOF

docker compose up --build
```

Or, if running alongside the main helpcore stack, add the service block to `docker-compose.prod.yml`:

```yaml
# In docker-compose.prod.yml, under services:
voice-kittentts:
  build: plugins/voice-kittentts/service
  ports:
    - "8080:8080"
  environment:
    HELPCORE_URL: http://helpcore:3000
    HELPCORE_TOKEN: "hcp_abc123defg456..."
    WHISPER_BIN: /whisper/whisper-cli
    WHISPER_MODEL: /whisper/models/ggml-base.en.bin
    KITTEN_VOICE: Jasper
  volumes:
    - ./plugins/voice-kittentts/whisper:/whisper:ro
  restart: unless-stopped
  depends_on:
    helpcore:
      condition: service_healthy
```

### 5. Verify the service is healthy

```bash
curl http://localhost:8080/health
# → {"status":"ok","version":"0.1.0"}
```

---

## Using the API

The voice service exposes three endpoints.

### `POST /transcribe`

Convert audio to text (STT only).

```bash
curl -F audio=@recording.wav http://localhost:8080/transcribe
# → {"text":"what is the weather like today"}
```

Accepts any audio format ffmpeg can read (WAV, MP3, M4A, OGG, WebM, FLAC).

### `POST /synthesize`

Convert text to audio (TTS only).

```bash
curl -X POST \
  -H "Content-Type: application/json" \
  -d '{"text": "Hello, this is a test.", "voice": "Jasper"}' \
  http://localhost:8080/synthesize \
  -o output.wav

# Play on macOS
afplay output.wav
```

Request body:
- `text` (required) — text to synthesise
- `voice` (optional) — voice name; defaults to `KITTEN_VOICE` env var

Returns `audio/wav`.

### `POST /voice/chat`

Full round-trip: audio → transcript → AI response → audio.

```bash
curl -F audio=@question.wav \
     http://localhost:8080/voice/chat \
     -o response.wav -D -

# Response headers include:
# X-Transcript: what the AI heard you say
# X-Response-Text: what the AI replied (text form)
# X-Conversation-Id: conversation ID (use for follow-up turns)

afplay response.wav
```

Form fields:
| Field | Required | Description |
|---|---|---|
| `audio` | yes | Audio file |
| `conversation_id` | no | Continue an existing conversation |
| `provider_id` | no | Use a specific helpcore provider |
| `model` | no | Use a specific model |
| `voice` | no | Override the default TTS voice |

To continue a conversation across turns:

```bash
# First turn
curl -F audio=@q1.wav http://localhost:8080/voice/chat -o r1.wav -D headers.txt
CONV_ID=$(grep X-Conversation-Id headers.txt | awk '{print $2}' | tr -d '\r')

# Follow-up turn
curl -F audio=@q2.wav -F conversation_id="$CONV_ID" \
     http://localhost:8080/voice/chat -o r2.wav
```

---

## Configuration

All settings are via environment variables:

| Variable | Default | Description |
|---|---|---|
| `HELPCORE_URL` | `http://localhost:3000` | helpcore server base URL |
| `HELPCORE_TOKEN` | *(required)* | `hcp_...` plugin token from `hc plugin token voice-kittentts` |
| `WHISPER_BIN` | `whisper-cli` | Path to the whisper.cpp binary inside the container |
| `WHISPER_MODEL` | `models/ggml-base.en.bin` | Path to the Whisper model file inside the container |
| `KITTEN_MODEL` | `KittenML/kitten-tts-mini-0.8` | KittenTTS HuggingFace model ID |
| `KITTEN_VOICE` | `Jasper` | Default TTS voice |
| `PORT` | `8080` | Service listening port |

A `.env` file in the service directory is also supported.

---

## Whisper model options

| Model | Size | Speed | Quality |
|---|---|---|---|
| `tiny.en` | 75 MB | Very fast | Basic |
| `base.en` | 150 MB | Fast | Good — **recommended** |
| `small.en` | 490 MB | Moderate | Better |
| `medium.en` | 1.5 GB | Slow | High |

Download with: `./models/download-ggml-model.sh base.en` (in the whisper.cpp repo).

---

## How the AI context changes

When the plugin is enabled, the following is appended to the system prompt for every chat message:

```
The user is using voice mode. Keep responses concise and spoken-word-friendly:
- No markdown formatting (no headers, bullet lists, code blocks, bold or italic)
- Natural sentences only — write how you would speak aloud
- Avoid lists; use "first... then... finally..." instead
- Keep responses under about 100 words unless the question genuinely requires more
```

This is loaded from `plugins/voice-kittentts/skill.md`. You can edit it to adjust the style.

Disable the plugin to turn this off:

```bash
hc plugin disable voice-kittentts
```

---

## Troubleshooting

**"whisper failed: error loading model"**  
Check that `WHISPER_MODEL` points to the correct `.bin` file inside the container, and that the whisper volume is mounted correctly.

**"HELPCORE_TOKEN: field required"**  
The token env var isn't set. Generate one with `hc plugin token voice-kittentts` and pass it in.

**KittenTTS slow to respond on first request**  
Normal — the model loads on first call. Subsequent requests are fast.

**No audio / silent WAV output**  
Check the `X-Response-Text` response header — if the text is there, TTS succeeded but the WAV may not be playing. Verify `afplay`/`aplay` supports 24 kHz float WAV.

**502 from `/voice/chat`**  
The voice service can't reach helpcore. Check `HELPCORE_URL` — if both are in Docker Compose, use the service name (`http://helpcore:3000`), not `localhost`.
