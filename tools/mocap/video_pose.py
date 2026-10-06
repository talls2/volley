"""Tracks a person's 3D pose through a video with Google's MediaPipe Pose
Landmarker (Apache 2.0) and writes it out for `retarget_mocap.py`.

For every frame: the 33 landmarks in metres around the hips ("world"
landmarks), and their 2D image positions, which give the body's travel and
height in the picture. Frames where nobody is found are left empty, and an
optional preview video draws the tracked skeleton over the footage.

Run with a Python environment that has MediaPipe (`python3 -m venv venv &&
venv/bin/pip install mediapipe opencv-python`, and the pose_landmarker_heavy
model from Google). It crashes on macOS (MediaPipe's Metal setup fails even on
the CPU); `vision_pose.swift` does the job there.

    venv/bin/python tools/mocap/video_pose.py VIDEO.mp4 OUT.json \\
        [--model pose_landmarker_heavy.task] [--start S] [--end S] [--preview OUT.mp4]
"""

import json
import sys
from pathlib import Path

import cv2
import mediapipe as mp
from mediapipe.tasks import python as tasks
from mediapipe.tasks.python import vision

# Pairs of landmarks drawn in the preview.
BONES = [(11, 12), (11, 13), (13, 15), (12, 14), (14, 16), (11, 23), (12, 24), (23, 24),
         (23, 25), (25, 27), (27, 31), (24, 26), (26, 28), (28, 32), (0, 11), (0, 12)]


def main():
    args = sys.argv[1:]
    value = lambda flag, default=None: args[args.index(flag) + 1] if flag in args else default
    video, out = Path(args[0]), Path(args[1])
    model = value("--model", str(Path("~/Downloads/volley-assets/mocap/pose_landmarker_heavy.task").expanduser()))
    start, end = float(value("--start", 0)), float(value("--end", 1e9))

    options = vision.PoseLandmarkerOptions(
        # The CPU: MediaPipe's Metal (GPU) path fails outside an app with a window.
        base_options=tasks.BaseOptions(model_asset_path=model, delegate=tasks.BaseOptions.Delegate.CPU),
        running_mode=vision.RunningMode.VIDEO,
        num_poses=1,
        min_pose_detection_confidence=0.5,
        min_pose_presence_confidence=0.5,
        min_tracking_confidence=0.6,
    )
    capture = cv2.VideoCapture(str(video))
    fps = capture.get(cv2.CAP_PROP_FPS) or 30.0
    width, height = int(capture.get(cv2.CAP_PROP_FRAME_WIDTH)), int(capture.get(cv2.CAP_PROP_FRAME_HEIGHT))
    preview = None
    if value("--preview"):
        preview = cv2.VideoWriter(value("--preview"), cv2.VideoWriter_fourcc(*"mp4v"), fps, (width // 2, height // 2))

    frames = []
    with vision.PoseLandmarker.create_from_options(options) as landmarker:
        index = -1
        while True:
            ok, image = capture.read()
            if not ok:
                break
            index += 1
            t = index / fps
            if t < start:
                continue
            if t > end:
                break
            rgb = cv2.cvtColor(image, cv2.COLOR_BGR2RGB)
            result = landmarker.detect_for_video(mp.Image(image_format=mp.ImageFormat.SRGB, data=rgb), int(t * 1000))
            if not result.pose_world_landmarks:
                frames.append(None)
            else:
                world = result.pose_world_landmarks[0]
                image_points = result.pose_landmarks[0]
                frames.append({
                    "world": [[p.x, p.y, p.z, p.visibility] for p in world],
                    "image": [[p.x, p.y] for p in image_points],
                })
            if preview is not None:
                small = cv2.resize(image, (width // 2, height // 2))
                if frames[-1]:
                    points = [(int(x * width / 2), int(y * height / 2)) for x, y in frames[-1]["image"]]
                    for a, b in BONES:
                        cv2.line(small, points[a], points[b], (0, 255, 255), 2)
                preview.write(small)
    if preview is not None:
        preview.release()
    found = sum(1 for f in frames if f)
    out.write_text(json.dumps({"fps": fps, "width": width, "height": height, "frames": frames}))
    print(f"wrote {out}: {len(frames)} frames at {fps:.0f} fps, person found in {found}")


main()
