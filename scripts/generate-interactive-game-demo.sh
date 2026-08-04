#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_DIR="$ROOT_DIR/frontend/public/interactive-game-demo/media"
mkdir -p "$OUTPUT_DIR"

make_clip() {
  local filename="$1"
  local background="$2"
  local title="$3"
  local subtitle="$4"
  local accent="$5"
  local duration="$6"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "color=c=${background}:s=1280x720:r=24:d=${duration}" \
    -vf "drawbox=x=70:y=70:w=1140:h=580:color=${accent}@0.18:t=6,drawbox=x='90+18*sin(2*PI*t/5)':y='430+20*cos(2*PI*t/4)':w=360:h=6:color=${accent}@0.72:t=fill,drawbox=x='760+80*cos(2*PI*t/6)':y='150+50*sin(2*PI*t/5)':w=180:h=180:color=${accent}@0.22:t=fill,drawbox=x='200+65*sin(2*PI*t/7)':y='180+30*cos(2*PI*t/6)':w=90:h=90:color=white@0.12:t=fill" \
    -an -c:v libx264 -preset veryfast -crf 26 -pix_fmt yuv420p -movflags +faststart "$OUTPUT_DIR/$filename"
}

make_clip "01-start.mp4" "0x17202d" "雾中醒来" "铃声从没有列车的站台传来" "0xd99b5b" 5
make_clip "02-bridge.mp4" "0x1c2730" "桥下的灯" "灯光在雾里留下第一条线索" "0x91b6d8" 5
make_clip "03-archive.mp4" "0x2c2527" "墙上的纸条" "湿透的墨迹指向旧档案馆" "0xd7856f" 5
make_clip "04-station.mp4" "0x20252b" "无声的站台" "两条路最终回到同一个选择" "0xa5c1a5" 5
make_clip "05-truth.mp4" "0x283b38" "看见天光" "完整证据让雾城迎来清晨" "0x83d6aa" 5
make_clip "06-dawn.mp4" "0x384033" "带她离开" "第一班晨车在雾外等待" "0xf0cb80" 5
make_clip "07-echo.mp4" "0x34252d" "回声尽头" "错误的脚步声吞没了出口" "0xe98978" 5
make_clip "08-silence.mp4" "0x202124" "沉入雾中" "纸条在雨水里失去了最后一个字" "0x8794a8" 5

echo "Generated 8 interactive game videos in $OUTPUT_DIR"
