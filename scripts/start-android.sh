#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
android_device="${ANDROID_DEVICE:-TB710FU}"
dev_port="${ANDROID_DEV_PORT:-1421}"
user_directory="$(dscl . -read "/Users/$(id -un)" NFSHomeDirectory | awk '{print $2}')"

collect_process_tree() {
  local parent_pid="$1"
  local child_pid
  while IFS= read -r child_pid; do
    [[ -n "$child_pid" ]] || continue
    collect_process_tree "$child_pid"
  done < <(pgrep -P "$parent_pid" 2>/dev/null || true)
  android_processes+=("$parent_pid")
}

stop_previous_android_session() {
  local root_pid
  local command_pattern="${project_root}/node_modules/.bin/../@tauri-apps/cli/tauri.js android dev"
  android_processes=()
  while IFS= read -r root_pid; do
    [[ -n "$root_pid" ]] || continue
    collect_process_tree "$root_pid"
  done < <(pgrep -f "$command_pattern" 2>/dev/null || true)
  ((${#android_processes[@]})) || return 0

  echo "检测到本工程旧的 Android 开发会话，正在停止后重新部署…"
  kill -TERM "${android_processes[@]}" 2>/dev/null || true
  for _ in {1..40}; do
    local active=0
    for root_pid in "${android_processes[@]}"; do
      if kill -0 "$root_pid" 2>/dev/null; then active=1; break; fi
    done
    ((active == 0)) && return 0
    sleep 0.25
  done
  echo "旧 Android 会话未能在 10 秒内退出，请先停止旧部署任务后重试。" >&2
  exit 1
}

stop_previous_android_session
cd "$project_root"
exec env \
  JAVA_HOME="${JAVA_HOME:-${user_directory}/.local/jdk-17/Contents/Home}" \
  ANDROID_HOME="${ANDROID_HOME:-${user_directory}/Library/Android/sdk}" \
  ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-${user_directory}/Library/Android/sdk}" \
  NDK_HOME="${NDK_HOME:-${user_directory}/Library/Android/sdk/ndk/29.0.14206865}" \
  ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-${user_directory}/Library/Android/sdk/ndk/29.0.14206865}" \
  npm run tauri -- android dev "$android_device" \
  --config "{\"build\":{\"beforeDevCommand\":\"npm run dev --workspace frontend -- --host 0.0.0.0 --port ${dev_port} --strictPort\",\"devUrl\":\"http://127.0.0.1:${dev_port}\"}}"
