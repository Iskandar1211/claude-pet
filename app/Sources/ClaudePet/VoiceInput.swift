@preconcurrency import AVFoundation
import Observation
import Speech

/// Диктовка для Quick Chat: микрофон → текст, распознавание только на этом Mac (DictationTranscriber, macOS 26+).
/// Русская модель скачивается системой один раз при первой записи; звук никуда не отправляется.
/// На macOS ниже 26 локального распознавания русского нет — кнопка неактивна, а не шлёт звук на серверы.
@MainActor
@Observable
final class VoiceInput {
    enum State: Equatable {
        case idle
        /// Первая запись: система скачивает модель распознавания.
        case preparing
        case listening
        case unavailable(String)
    }

    private(set) var state: State = .idle
    /// Весь распознанный за запись текст: подтверждённая часть + текущая догадка.
    var onTranscript: ((String) -> Void)?

    private static let locale = Locale(identifier: "ru_RU")
    /// `DictationSession`; `AnyObject`, потому что сам тип есть только в macOS 26.
    private var session: AnyObject?

    init() {
        if #unavailable(macOS 26) {
            state = .unavailable("Голосовой ввод работает на macOS 26 и новее")
        }
    }

    func toggle() {
        switch state {
        case .idle: Task { await start() }
        case .listening, .preparing: stop()
        case .unavailable: break
        }
    }

    func stop() {
        guard #available(macOS 26, *), let session = session as? DictationSession else { return }
        self.session = nil
        state = .idle
        Task { await session.finish() }
    }

    private func start() async {
        guard #available(macOS 26, *) else { return }
        guard await AVCaptureDevice.requestAccess(for: .audio) else {
            state = .unavailable("Нет доступа к микрофону: Системные настройки → Конфиденциальность → Микрофон")
            return
        }
        // `assetInstallationRequest` возвращает запрос и для уже скачанной модели, поэтому «загружаю»
        // решаем по списку установленных языков — иначе подпись мелькала бы при каждой записи.
        let installed = await DictationTranscriber.installedLocales.contains { $0.identifier == Self.locale.identifier }
        state = installed ? .listening : .preparing
        let session = DictationSession(locale: Self.locale) { [weak self] text in
            Task { @MainActor in self?.onTranscript?(text) }
        }
        self.session = session
        do {
            try await session.start()
            // Пока качалась модель, пользователь мог нажать «стоп».
            guard self.session === session else { return await session.finish() }
            state = .listening
        } catch {
            self.session = nil
            await session.finish()
            NSLog("ClaudePet: диктовка не запустилась: \(error)")
            state = .unavailable("Не удалось включить диктовку: \(error.localizedDescription)")
        }
    }
}

/// Одна запись: AVAudioEngine → SpeechAnalyzer → текст. Живёт от нажатия 🎤 до остановки.
@available(macOS 26, *)
@MainActor
private final class DictationSession {
    private let transcriber: DictationTranscriber
    private let analyzer: SpeechAnalyzer
    private let engine = AVAudioEngine()
    private let onText: @Sendable (String) -> Void
    private var input: AsyncStream<AnalyzerInput>.Continuation?
    private var results: Task<Void, Never>?

    init(locale: Locale, onText: @escaping @Sendable (String) -> Void) {
        transcriber = DictationTranscriber(locale: locale, preset: .progressiveShortDictation)
        analyzer = SpeechAnalyzer(modules: [transcriber])
        self.onText = onText
    }

    func start() async throws {
        if let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
            try await request.downloadAndInstall()
        }
        guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber]) else {
            throw VoiceError.noAudioFormat
        }
        let (stream, input) = AsyncStream<AnalyzerInput>.makeStream()
        self.input = input

        let transcriber = transcriber, onText = onText
        results = Task {
            var finalized = ""
            do {
                for try await result in transcriber.results {
                    let text = String(result.text.characters)
                    if result.isFinal {
                        finalized += text
                        onText(finalized)
                    } else {
                        onText(finalized + text)
                    }
                }
            } catch {
                NSLog("ClaudePet: распознавание прервано: \(error)")
            }
        }

        let mic = engine.inputNode.outputFormat(forBus: 0)
        guard let converter = AVAudioConverter(from: mic, to: format) else { throw VoiceError.noAudioFormat }
        engine.inputNode.installTap(onBus: 0, bufferSize: 4096, format: mic, block: Self.tap(converter, format, input))
        engine.prepare()
        try engine.start()
        try await analyzer.start(inputSequence: stream)
    }

    /// Останавливает микрофон и дожидается последних результатов.
    func finish() async {
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        input?.finish()
        input = nil
        try? await analyzer.finalizeAndFinishThroughEndOfInput()
        await results?.value
    }

    /// Вызывается на аудиопотоке, поэтому вне главного актора.
    private nonisolated static func tap(
        _ converter: AVAudioConverter, _ format: AVAudioFormat, _ input: AsyncStream<AnalyzerInput>.Continuation
    ) -> AVAudioNodeTapBlock {
        { buffer, _ in
            guard let converted = convert(buffer, with: converter, to: format) else { return }
            input.yield(AnalyzerInput(buffer: converted))
        }
    }

    /// Частота и формат микрофона обычно не совпадают с тем, что ждёт модель.
    private nonisolated static func convert(_ buffer: AVAudioPCMBuffer, with converter: AVAudioConverter, to format: AVAudioFormat) -> AVAudioPCMBuffer? {
        let ratio = format.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount((Double(buffer.frameLength) * ratio).rounded(.up))
        guard let output = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity) else { return nil }
        nonisolated(unsafe) var consumed = false // блок конвертера вызывается синхронно, внутри convert
        var error: NSError?
        converter.convert(to: output, error: &error) { _, status in
            if consumed {
                status.pointee = .noDataNow
                return nil
            }
            consumed = true
            status.pointee = .haveData
            return buffer
        }
        return error == nil ? output : nil
    }
}

private enum VoiceError: LocalizedError {
    case noAudioFormat

    var errorDescription: String? {
        switch self {
        case .noAudioFormat: "микрофон выдаёт неподходящий формат звука"
        }
    }
}
