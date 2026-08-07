#!/usr/bin/env bash

set -eu

: "${DASHSCOPE_API_KEY:?请先设置 DASHSCOPE_API_KEY}"

curl --location 'https://dashscope.aliyuncs.com/api/v1/services/aigc/video-generation/video-synthesis' \
    -H 'X-DashScope-Async: enable' \
    -H "Authorization: Bearer ${DASHSCOPE_API_KEY}" \
    -H 'Content-Type: application/json' \
    -d '{
    "model": "wan2.7-r2v-2026-06-12",
    "input": {
        "prompt": "图1 中的猫在草地上奔跑。",
        "media": [
            {
                "type": "reference_image",
                "url": "https://cdn.translate.alibaba.com/r/wanx-demo-1.png"
            }
        ]
    },
    "parameters": {
        "resolution": "720P",
        "ratio": "16:9",
        "duration": 5
    }
}'
