#!/bin/bash
# Test image-to-image / edit generation (img2img)
# Usage:
#   ./test_i2i.sh [IMAGE_PATH_OR_URL]
# Examples:
#   ./test_i2i.sh input.jpg
#   ./test_i2i.sh https://example.com/photo.jpg

IMAGE="${1:-input.jpg}"

if [ -f "$IMAGE" ]; then
  # If local file exists, use standard OpenAI multipart upload
  curl -X POST http://127.0.0.1:8190/v1/images/edits \
    -F "image=@$IMAGE" \
    -F "prompt=Transform the scene with vibrant autumn colors, falling cherry blossoms, 8k masterpiece" \
    -F "size=512x512" \
    -F "response_format=b64_json"
else
  # Otherwise send JSON payload (supports URL, OpenWebUI file path, or data URI)
  curl -X POST http://127.0.0.1:8190/v1/images/edits \
    -H "Content-Type: application/json" \
    -d '{
      "prompt": "Transform the scene with vibrant autumn colors, falling cherry blossoms, 8k masterpiece",
      "image": "'"$IMAGE"'",
      "size": "512x512",
      "response_format": "b64_json"
    }'
fi
