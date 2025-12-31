#!/usr/bin/env python3
import http.client
import json

conn = http.client.HTTPConnection("127.0.0.1", 8999)

headers = {
    "Content-Type": "application/json",
    "Authorization": "Bearer Proxycast-key11"
}

data = {
    "model": "claude-opus-4-5-20251101",
    "messages": [{"role": "user", "content": "你好"}],
    "stream": True
}

print("发送流式请求...")
print(f"请求数据: {json.dumps(data, ensure_ascii=False)}\n")
conn.request("POST", "/v1/chat/completions", json.dumps(data), headers)

response = conn.getresponse()
print(f"HTTP 状态码: {response.status}")
print(f"响应头: {dict(response.getheaders())}\n")

print("开始接收流式数据:\n")
chunk_count = 0
content_buffer = ""

while True:
    line = response.readline()
    if not line:
        break

    chunk_count += 1
    decoded = line.decode('utf-8').strip()

    if not decoded:
        continue

    print(f"Chunk {chunk_count}: {decoded[:150]}...")

    if decoded.startswith('data: '):
        json_str = decoded[6:]
        if json_str == '[DONE]':
            print("  → 流结束")
            break

        try:
            data_obj = json.loads(json_str)
            if 'choices' in data_obj and data_obj['choices']:
                delta = data_obj['choices'][0].get('delta', {})
                content = delta.get('content', '')
                if content:
                    content_buffer += content
                    print(f"  ✓ 内容: {content}")
        except json.JSONDecodeError as e:
            print(f"  ✗ JSON 解析失败: {e}")

print(f"\n总共接收 {chunk_count} 个数据块")
print(f"累积内容: {content_buffer}")

conn.close()
