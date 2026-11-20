#!/usr/bin/env bash
set -eu
w=~/ext/work/site/r5
export TMPDIR="$w/tmp"
d=docs/images/recordings
# Assemble lossless screencast source frames with the viewer display.
open=$(ls -td "$w"/open.mp4.frames-* | head -1)
back=$(ls -td "$w"/return.mp4.frames-* | head -1)
# CDP timestamps include machine load. Fit only the launcher shots to their
# edit lengths, keeping the live viewer at its recorded speed.
open_time=$(awk '$1=="duration"{s+=$2} END{print s}' "$open/list.txt")
back_time=$(awk '$1=="duration"{s+=$2} END{print s}' "$back/list.txt")
for width in 1280 1960; do
  suffix=""; height=862
  if [[ $width == 1960 ]]; then suffix="-2x"; height=1320; fi
  ffmpeg -y -loglevel error -filter_complex_threads 3 -f concat -safe 0 -i "$open/list.txt" -i "$w/session.mkv" -f concat -safe 0 -i "$back/list.txt" -loop 1 -t 0.375 -i "$open/00000.png" -filter_complex "[0:v]setpts=(PTS-STARTPTS)*6.125/$open_time,fps=24,scale=$width:$height:flags=lanczos,setsar=1,format=yuv420p[a];[1:v]trim=end=6.875,fps=24,scale=$width:$height:flags=lanczos,setsar=1,format=yuv420p[b];[2:v]trim=start=0.20,setpts=(PTS-STARTPTS)*2.208/($back_time-0.20),fps=24,scale=$width:$height:flags=lanczos,setsar=1,format=yuv420p[c];[3:v]fps=24,scale=$width:$height:flags=lanczos,setsar=1,format=yuv420p[d];[a][b][c][d]concat=n=4:v=1:a=0" -c:v libx264 -threads 5 -preset slow -crf 25 -movflags +faststart "$d/hero$suffix.mp4"
  ffmpeg -y -loglevel error -i "$d/hero$suffix.mp4" -c:v libvpx-vp9 -cpu-used 5 -row-mt 1 -threads 5 -crf 42 -b:v 0 "$d/hero$suffix.webm"
done
ffmpeg -y -loglevel error -i "$d/hero.mp4" -vf 'fps=8,scale=1100:-2' -loop 0 -compression_level 6 -quality 50 docs/images/hero.webp
cp "$w/hover.png" docs/images/hero.png
magick "$w/hover.png" -quality 91 "$d/poster.webp"
magick "$w/plain.png" "$open/00000.png" +append ~/ext/work/site/shots/r5-real-vs-video.png
ffmpeg -y -loglevel error -t 5.25 -i "$w/session.mkv" -vf "crop=834:456:72:286" -c:v libx264 -crf 18 -pix_fmt yuv420p -movflags +faststart "$d/closeup.mp4"
magick "$w/result.png" -crop 834x456+72+286 +repage -quality 94 "$d/closeup.webp"
