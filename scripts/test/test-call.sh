curl https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${API_KEY}" \
  -d '{
    "model": "doubao-seedance-2.0",
    "content": [
      {
        "type": "text",
        "text": "男主角御剑在云间穿行"
      },
      {
        "type": "image_url",
        "image_url": {
          "url": "https://monkey-1256112104.cos.ap-chengdu.myqcloud.com/media/f06107d668a64ae8ae72572743f1f77c.png"
        }
      }
    ],
    "generate_audio": true,
    "ratio": "adaptive",
    "duration": 5,
    "watermark": false
  }'
