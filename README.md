# comfy-api-openai-proxy

A lightweight Rust reverse proxy built with **Axum** and **Reqwest** that accepts standard **OpenAI Image Generation and Edit API** requests (`/v1/images/generations` and `/v1/images/edits`) and translates them to the official **Comfy API v2** contract (supported by `comfy-api-proxy`, Comfy Cloud, and dedicated Comfy deployments).

---

## Features

- **OpenAI Standard Compatibility**:
  - `POST /v1/images/generations`: Text-to-Image with JSON payload.
  - `POST /v1/images/edits`: Image-to-Image / Edit with `multipart/form-data`.
  - Supports `response_format`: `"url"` and `"b64_json"`.
  - Supports custom dimensions (`size`), prompt, and models.
- **Comfy API v2 Native**:
  - Automatically uploads input images to `POST /api/v2/assets`.
  - References inputs in workflows using the standard `core/ASSET` specification.
  - Submits jobs to `POST /api/v2/jobs` with generated `Idempotency-Key` headers.
  - Polls `GET /api/v2/jobs/{id}` until terminal status (`succeeded`, `failed`, `expired`, `canceled`).
  - Downloads generated assets from `GET /api/v2/assets/{id}/content`.
- **Workflow Templating**:
  - Customizable API-format workflow templates in `templates/txt2img.json` and `templates/img2img.json`.
  - Automatically randomizes seeds and injects prompts, dimensions, and checkpoint loaders.
- **Resilient & Fast**:
  - Built with Rust, Axum, and Tokio for asynchronous execution and high throughput.

---

## Architecture Flow

```
                      +------------------------------------------+
                      |       Rust Reverse Proxy (Axum)          |
                      |                                          |
  Client (OpenAI SDK) |  POST /v1/images/generations (JSON)      |
--------------------->|  POST /v1/images/edits (Multipart)       |
                      |                                          |
                      |  1. Parse request & validate             |
                      |  2. If edit: Upload asset to v2          |
                      |  3. Hydrate workflow template            |
                      |  4. Submit job via POST /api/v2/jobs     |
                      |  5. Poll GET /api/v2/jobs/{id}           |
                      |  6. Retrieve asset & format response     |
                      +--------------------+---------------------+
                                           | Reqwest HTTP Client
                                           v
                      +------------------------------------------+
                      |           Comfy API v2 Surface           |
                      |    (Local comfy-api-proxy / Cloud)       |
                      +------------------------------------------+
```

---

## Configuration

Set via environment variables:

| Variable | Description | Default |
| :--- | :--- | :--- |
| `COMFY_BASE_URL` | Comfy API v2 base URL | `http://127.0.0.1:8189` |
| `COMFY_API_KEY` | Optional Bearer token (for Comfy Cloud or authenticated proxies) | _None_ |
| `HOST` | Proxy bind host | `0.0.0.0` |
| `PORT` | Proxy bind port | `8190` |
| `POLL_INTERVAL_MS` | Job polling delay in milliseconds | `500` |
| `JOB_TIMEOUT_SECS` | Maximum seconds before job timeout | `180` |
| `DEFAULT_CHECKPOINT` | Override default checkpoint name in template | _None_ |
| `TXT2IMG_TEMPLATE_PATH` | Path to txt2img workflow JSON | `templates/txt2img.json` |
| `IMG2IMG_TEMPLATE_PATH` | Path to img2img workflow JSON | `templates/img2img.json` |
| `TXT2IMG_PROMPT_NODE_ID` | Optional node ID for txt2img prompt (e.g. `459:471` or `6`) | _Auto-detect_ |
| `IMG2IMG_PROMPT_NODE_ID` | Optional node ID for img2img prompt (e.g. `459:471` or `6`) | _Auto-detect_ |

---

## Quickstart

### 1. Start ComfyUI and comfy-api-proxy

Run your self-hosted ComfyUI instance (typically on port 8188) and start `comfy-api-proxy`:

```bash
pip install comfy-api-proxy
comfy-api-proxy
```
*By default, `comfy-api-proxy` listens on `http://127.0.0.1:8189`.*

### 2. Run the Reverse Proxy

#### Option A: Running with Docker Compose (Recommended for Containers)

Ensure Docker is running, then run:

```bash
docker compose up -d --build
```

- Maps `http://localhost:8190` to the container.
- Connects automatically to `comfy-api-proxy` running on your host machine via `http://host.docker.internal:8189`.
- Mounts `./templates` into `/app/templates` so you can customize workflows without rebuilding the image.

To follow container logs:
```bash
docker compose logs -f
```

To stop:
```bash
docker compose down
```

#### Option B: Running with Cargo (Native)

```bash
cargo run --release
```

---

## Usage Examples

### 1. Text-to-Image Generation (curl)

```bash
curl http://localhost:8190/v1/images/generations \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "A tranquil Japanese garden with cherry blossoms in spring, 8k masterpiece",
    "size": "512x512",
    "response_format": "b64_json"
  }'
```

### 2. Image Edit (curl)

```bash
curl http://localhost:8190/v1/images/edits \
  -F "image=@input.png" \
  -F "prompt=Add snow and winter lighting to the scene" \
  -F "response_format=url"
```

### 3. OpenAI Python SDK

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:8190/v1",
    api_key="not-needed" # or your COMFY_API_KEY if authenticated
)

# Text-to-Image
response = client.images.generate(
    model="v1-5-pruned-emaonly.ckpt",
    prompt="A futuristic neon city at night",
    size="512x512",
    response_format="b64_json",
    n=1
)
print(response.data[0].b64_json[:50])

# Image Edit
with open("photo.png", "rb") as image_file:
    edit_response = client.images.edit(
        image=image_file,
        prompt="Make it oil painting style",
        response_format="url"
    )
    print(edit_response.data[0].url)
```

---

## Workflow Customization

Templates are standard ComfyUI API-format JSON graphs:
- `templates/txt2img.json`: Used for text generations.
- `templates/img2img.json`: Used for image edits.

To use your own workflow:
1. In ComfyUI, configure your workflow.
2. Enable **Dev Mode** in ComfyUI settings, then click **Save (API Format)**.
3. Save the JSON file to `templates/txt2img.json` or `templates/img2img.json`.
