// Tracks a person's 3D pose through a video with Apple's Vision framework
// (VNDetectHumanBodyPose3DRequest, macOS 14+) and writes it out for
// `retarget_mocap.py`: for every frame, 17 joints in metres around the root
// (the hips' centre), and their 2D image positions.
//
// Build and run:
//
//     swiftc -O tools/mocap/vision_pose.swift -o ~/Downloads/volley-mocap/vision_pose
//     ~/Downloads/volley-mocap/vision_pose VIDEO.mp4 OUT.json [--start S] [--end S]

import AVFoundation
import Foundation
import Vision

let joints: [VNHumanBodyPose3DObservation.JointName] = [
    .root, .leftHip, .leftKnee, .leftAnkle, .rightHip, .rightKnee, .rightAnkle,
    .spine, .centerShoulder, .centerHead, .topHead,
    .leftShoulder, .leftElbow, .leftWrist, .rightShoulder, .rightElbow, .rightWrist,
]

func argument(_ flag: String) -> String? {
    guard let i = CommandLine.arguments.firstIndex(of: flag), i + 1 < CommandLine.arguments.count else { return nil }
    return CommandLine.arguments[i + 1]
}

let args = CommandLine.arguments
guard args.count >= 3 else {
    print("usage: vision_pose VIDEO OUT.json [--start S] [--end S]")
    exit(1)
}
let start = Double(argument("--start") ?? "0") ?? 0
let end = Double(argument("--end") ?? "1e9") ?? 1e9

let asset = AVURLAsset(url: URL(fileURLWithPath: args[1]))
let semaphore = DispatchSemaphore(value: 0)
var track: AVAssetTrack?
Task {
    track = try? await asset.loadTracks(withMediaType: .video).first
    semaphore.signal()
}
semaphore.wait()
guard let video = track else {
    print("no video track")
    exit(1)
}
let reader = try AVAssetReader(asset: asset)
let output = AVAssetReaderTrackOutput(track: video, outputSettings: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA])
reader.add(output)
reader.startReading()
let fps = Double(video.nominalFrameRate)
let size = video.naturalSize

var frames: [Any] = []
var found = 0
while let sample = output.copyNextSampleBuffer() {
    let t = CMSampleBufferGetPresentationTimeStamp(sample).seconds
    if t < start { continue }
    if t > end { break }
    guard let buffer = CMSampleBufferGetImageBuffer(sample) else { frames.append(NSNull()); continue }
    let request = VNDetectHumanBodyPose3DRequest()
    let handler = VNImageRequestHandler(cvPixelBuffer: buffer, options: [:])
    do {
        try handler.perform([request])
    } catch {
        frames.append(NSNull())
        continue
    }
    guard let body = request.results?.first else { frames.append(NSNull()); continue }
    var world: [[Double]] = []
    var image: [[Double]] = []
    for joint in joints {
        guard let point = try? body.recognizedPoint(joint) else {
            world.append([0, 0, 0, 0])
            image.append([0, 0])
            continue
        }
        let p = point.position.columns.3
        world.append([Double(p.x), Double(p.y), Double(p.z), 1])
        if let flat = try? body.pointInImage(joint) {
            image.append([Double(flat.location.x), 1 - Double(flat.location.y)])
        } else {
            image.append([0, 0])
        }
    }
    frames.append(["world": world, "image": image, "height": Double(body.bodyHeight)])
    found += 1
}

let names = joints.map { $0.rawValue.rawValue }
let result: [String: Any] = ["fps": fps, "width": Double(size.width), "height": Double(size.height), "joints": names, "frames": frames]
let data = try JSONSerialization.data(withJSONObject: result)
try data.write(to: URL(fileURLWithPath: args[2]))
print("wrote \(args[2]): \(frames.count) frames at \(Int(fps)) fps, person found in \(found)")
