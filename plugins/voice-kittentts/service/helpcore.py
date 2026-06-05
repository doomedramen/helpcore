"""Async SSE client for the helpcore /chat endpoint."""

import json

import httpx

from config import settings


async def chat(
    message: str,
    conversation_id: str | None = None,
    provider_id: str | None = None,
    model: str | None = None,
) -> tuple[str, str]:
    """Send a message to helpcore and collect the full streamed response.

    Returns:
        (full_response_text, conversation_id)

    Raises:
        RuntimeError: if the server returns an error event or the stream ends
                      without a done event.
    """
    body: dict = {"message": message}
    if conversation_id:
        body["conversation_id"] = conversation_id
    if provider_id:
        body["provider_id"] = provider_id
    if model:
        body["model"] = model

    headers = {
        "Authorization": f"Bearer {settings.helpcore_token}",
        "Content-Type": "application/json",
        "Accept": "text/event-stream",
    }

    full_text = ""
    returned_conv_id = conversation_id or ""

    async with httpx.AsyncClient(timeout=120.0) as client:
        async with client.stream(
            "POST",
            f"{settings.helpcore_url}/chat",
            headers=headers,
            json=body,
        ) as resp:
            resp.raise_for_status()

            event_type = ""
            async for line in resp.aiter_lines():
                line = line.rstrip()

                if line.startswith("event:"):
                    event_type = line[len("event:"):].strip()
                elif line.startswith("data:"):
                    data_str = line[len("data:"):].strip()
                    if event_type == "chunk":
                        try:
                            payload = json.loads(data_str)
                            full_text += payload.get("delta", "")
                        except json.JSONDecodeError:
                            # Plain-text delta fallback
                            full_text += data_str
                    elif event_type == "done":
                        try:
                            payload = json.loads(data_str)
                            returned_conv_id = payload.get("conversation_id", returned_conv_id)
                        except json.JSONDecodeError:
                            pass
                        break
                    elif event_type == "error":
                        raise RuntimeError(f"helpcore error: {data_str}")
                elif line == "":
                    # Blank line resets event type for next event
                    event_type = ""

    if not full_text:
        raise RuntimeError("helpcore returned an empty response")

    return full_text, returned_conv_id
