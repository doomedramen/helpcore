"""Service configuration via environment variables (12-factor)."""

from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    # helpcore connection
    helpcore_url: str = "http://localhost:3000"
    helpcore_token: str  # required — no default; must be set to an hcp_... token

    # Whisper STT
    whisper_bin: str = "whisper-cli"
    whisper_model: str = "models/ggml-base.en.bin"

    # KittenTTS
    kitten_model: str = "KittenML/kitten-tts-mini-0.8"
    kitten_voice: str = "Jasper"

    # Service
    port: int = 8080

    class Config:
        env_file = ".env"
        env_file_encoding = "utf-8"


settings = Settings()
