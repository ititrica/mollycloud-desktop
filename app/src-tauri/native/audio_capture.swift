// ScreenCaptureKit system output -> little-endian mono Float32 PCM on stdout.
// Only instantiated after the user enables audio interaction in MollyCloud.
import Foundation
import ScreenCaptureKit
import CoreMedia
import AudioToolbox
import Darwin

func fail(_ message: String, code: Int32 = 2) -> Never {
    FileHandle.standardError.write(Data(message.utf8))
    exit(code)
}

@available(macOS 13.0, *)
final class AudioOutput: NSObject, SCStreamOutput, SCStreamDelegate {
    private var pending: [Float] = []
    var capture: SCStream?

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        fail("系统音频捕获已停止：\(error.localizedDescription)", code: 4)
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of outputType: SCStreamOutputType) {
        guard outputType == .audio, CMSampleBufferIsValid(sampleBuffer),
              let format = CMSampleBufferGetFormatDescription(sampleBuffer),
              let description = CMAudioFormatDescriptionGetStreamBasicDescription(format),
              description.pointee.mFormatID == kAudioFormatLinearPCM,
              description.pointee.mFormatFlags & kAudioFormatFlagIsFloat != 0,
              description.pointee.mBitsPerChannel == 32 else { return }
        var needed = 0
        var retained: CMBlockBuffer?
        let probe = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(sampleBuffer,
            bufferListSizeNeededOut: &needed, bufferListOut: nil, bufferListSize: 0,
            blockBufferAllocator: nil, blockBufferMemoryAllocator: nil,
            flags: kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment, blockBufferOut: &retained)
        guard probe == noErr, needed > 0 else { return }
        let storage = UnsafeMutableRawPointer.allocate(byteCount: needed, alignment: 16)
        defer { storage.deallocate() }
        let list = storage.bindMemory(to: AudioBufferList.self, capacity: 1)
        guard CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(sampleBuffer,
            bufferListSizeNeededOut: nil, bufferListOut: list, bufferListSize: needed,
            blockBufferAllocator: nil, blockBufferMemoryAllocator: nil,
            flags: kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment, blockBufferOut: &retained) == noErr else { return }
        let buffers = UnsafeMutableAudioBufferListPointer(list)
        let frames = CMSampleBufferGetNumSamples(sampleBuffer)
        guard frames > 0 else { return }
        let channels = buffers.reduce(0) { $0 + Int($1.mNumberChannels) }
        guard channels > 0 else { return }
        for frame in 0..<frames {
            var mono: Float = 0
            for buffer in buffers {
                guard let data = buffer.mData else { continue }
                let channelCount = Int(buffer.mNumberChannels)
                let samples = data.assumingMemoryBound(to: Float.self)
                let capacity = Int(buffer.mDataByteSize) / MemoryLayout<Float>.size
                for channel in 0..<channelCount {
                    let index = frame * channelCount + channel
                    if index < capacity { mono += samples[index] }
                }
            }
            let value = mono / Float(channels)
            pending.append(value.isFinite ? min(1, max(-1, value)) : 0)
            if pending.count == 1024 {
                do { try pending.withUnsafeBytes { try FileHandle.standardOutput.write(contentsOf: Data($0)) } }
                catch { exit(0) }
                pending.removeAll(keepingCapacity: true)
            }
        }
    }

    func start() async {
        do {
            let available = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
            guard let display = available.displays.first else { fail("没有可用于系统声音捕获的显示器", code: 3) }
            let filter = SCContentFilter(display: display, excludingApplications: [], exceptingWindows: [])
            let configuration = SCStreamConfiguration()
            configuration.width = 2
            configuration.height = 2
            configuration.minimumFrameInterval = CMTime(value: 1, timescale: 1)
            configuration.queueDepth = 3
            configuration.showsCursor = false
            configuration.capturesAudio = true
            configuration.excludesCurrentProcessAudio = false
            configuration.sampleRate = 48000
            configuration.channelCount = 2
            let stream = SCStream(filter: filter, configuration: configuration, delegate: self)
            capture = stream
            try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: DispatchQueue(label: "com.mollycloud.system-audio"))
            try await stream.startCapture()
        } catch {
            fail("无法捕获系统声音：\(error.localizedDescription)。请在系统设置 → 隐私与安全性 → 屏幕与系统音频录制中允许 MollyCloud，然后重新开启音频互动。")
        }
    }
}

if #available(macOS 13.0, *) {
    signal(SIGPIPE, SIG_IGN)
    let parent = CommandLine.arguments.dropFirst().first.flatMap(Int32.init) ?? getppid()
    let parentMonitor = DispatchSource.makeTimerSource(queue: .global())
    parentMonitor.schedule(deadline: .now() + 1, repeating: 1)
    parentMonitor.setEventHandler { if kill(parent, 0) != 0 { exit(0) } }
    parentMonitor.resume()
    let output = AudioOutput()
    Task { await output.start() }
    RunLoop.main.run()
} else {
    fail("系统声音互动需要 macOS 13 或更新版本。其余桌宠功能仍可正常使用。")
}
