import AVFoundation
import Observation

/// Ответ Claude вслух системным русским голосом. Читается только обычный текст (см. `speakable`).
@MainActor
@Observable
final class VoiceOutput {
    private(set) var isSpeaking = false

    @ObservationIgnored private let synthesizer = AVSpeechSynthesizer()
    @ObservationIgnored private lazy var delegate = Delegate { [weak self] speaking in
        Task { @MainActor in self?.isSpeaking = speaking }
    }

    func speak(_ text: String) {
        let text = Self.speakable(text)
        guard !text.isEmpty else { return }
        synthesizer.delegate = delegate
        synthesizer.stopSpeaking(at: .immediate)
        let utterance = AVSpeechUtterance(string: text)
        utterance.voice = AVSpeechSynthesisVoice(language: "ru-RU")
        synthesizer.speak(utterance)
    }

    func stop() {
        synthesizer.stopSpeaking(at: .immediate)
    }

    /// Чистая функция: Markdown ответа → текст для чтения. Блоки кода, разделители и таблицы пропускаются —
    /// слушать их бессмысленно; строчная разметка (`**жирный**`, `код`) снимается, текст остаётся.
    static func speakable(_ markdown: String) -> String {
        Markdown.blocks(markdown).compactMap { block -> String? in
            switch block {
            case .paragraph(let text):
                let lines = text.split(separator: "\n").filter { !$0.hasPrefix("|") }
                return lines.isEmpty ? nil : plain(lines.joined(separator: "\n"))
            case .heading(_, let text), .listItem(_, _, let text), .quote(let text):
                return plain(text)
            case .code, .rule:
                return nil
            }
        }
        .filter { !$0.isEmpty }
        .joined(separator: "\n")
    }

    private static func plain(_ text: String) -> String {
        String(Markdown.inline(text).characters).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Делегат синтезатора вызывается не на главном потоке — только сообщает «говорит / замолчал».
    private final class Delegate: NSObject, AVSpeechSynthesizerDelegate, @unchecked Sendable {
        private let onChange: @Sendable (Bool) -> Void

        init(onChange: @escaping @Sendable (Bool) -> Void) {
            self.onChange = onChange
        }

        func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didStart utterance: AVSpeechUtterance) {
            onChange(true)
        }

        func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
            onChange(synthesizer.isSpeaking)
        }

        func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
            onChange(synthesizer.isSpeaking)
        }
    }
}
