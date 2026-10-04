curl http://localhost:3000/v1/images/generations \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "A tranquil Japanese garden with cherry blossoms in spring, 8k masterpiece",
    "size": "512x512",
    "response_format": "b64_json"
  }'
