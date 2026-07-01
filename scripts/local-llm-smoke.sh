#!/usr/bin/env bash
# Local-only LM Studio smoke test. No GitHub secret or network tunnel required.
set -euo pipefail

BASE_URL="${LM_STUDIO_BASE_URL:-http://127.0.0.1:1234/v1}"
MODEL="${LM_STUDIO_MODEL:-google/gemma-4-26b-a4b-qat}"
EXPECTED="bolt local llm ok"

if ! command -v curl >/dev/null 2>&1; then
  echo "error: curl is required" >&2
  exit 127
fi

payload="$(python3 - <<'PY'
import json, os
model = os.environ.get("LM_STUDIO_MODEL", "google/gemma-4-26b-a4b-qat")
print(json.dumps({
    "model": model,
    "messages": [
        {
            "role": "system",
            "content": "Answer with exactly these four words and nothing else: bolt local llm ok",
        },
        {"role": "user", "content": "Smoke test"},
    ],
    "temperature": 0,
    "max_tokens": 256,
}))
PY
)"

response="$(curl -fsS --max-time 180 "$BASE_URL/chat/completions" \
  -H 'Content-Type: application/json' \
  -d "$payload")"

content="$(RESPONSE="$response" python3 - <<'PY'
import json, os
j = json.loads(os.environ["RESPONSE"])
print(j["choices"][0]["message"].get("content", ""))
PY
)"

if [[ "$content" != "$EXPECTED" ]]; then
  echo "error: unexpected local LLM response" >&2
  echo "expected: $EXPECTED" >&2
  echo "actual:   $content" >&2
  exit 1
fi

echo "$content"
