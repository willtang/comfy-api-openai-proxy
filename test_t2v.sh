#!/bin/bash
curl http://127.0.0.1:8190/v1/videos \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "A tranquil Japanese garden with cherry blossoms in spring, cinematic video",
    "size": "512x512",
    "seconds": 3,
    "fps": 24
  }'
