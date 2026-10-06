#!/bin/bash
# Test first-frame image-to-video generation (img2vid)
# Usage:
#   ./test_i2v.sh [IMAGE_PATH_OR_URL]
# Examples:
#   ./test_i2v.sh input.jpg
#   ./test_i2v.sh https://example.com/start_frame.jpg

IMAGE="${1:-input.jpg}"

if [ -f "$IMAGE" ]; then
  # If local file exists, use standard OpenAI multipart upload
  curl -X POST http://127.0.0.1:8190/v1/videos \
    -F "image=@$IMAGE" \
    -F "prompt=The dog starts running, cinematic lighting" \
    -F "size=512x512" \
    -F "seconds=4" \
    -F "fps=24"
else
  # Otherwise send JSON payload (supports URL, OpenWebUI file path, or data URI)
  curl -X POST http://127.0.0.1:8190/v1/videos \
    -H "Content-Type: application/json" \
    -d '{
      "prompt": "The dog starts running, cinematic lighting",
      "input_reference": "'"$IMAGE"'",
      "size": "512x512",
      "seconds": 4,
      "fps": 24
    }'
fi
