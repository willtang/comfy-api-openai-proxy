curl http://192.168.0.112:8190/v1/images/generations \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "A tranquil Japanese garden with cherry blossoms in spring, 8k masterpiece",
    "size": "512x512",
    "response_format": "b64_json"
  }'
