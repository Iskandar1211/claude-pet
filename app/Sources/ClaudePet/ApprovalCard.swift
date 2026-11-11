import SwiftUI

/// Карточка одобрения: один вид для чата и для пузыря у питомца. Тексты из сессии недоверенные —
/// только через `UntrustedText`. Ответ уходит в ту сессию, чей запрос показан.
struct ApprovalCard: View {
    let model: QuickChatModel
    let session: BridgeSession
    let pending: BridgeSession.PendingPermission
    /// Пузырь: ниже превью, в заголовке — из какой сессии запрос.
    var compact = false
    /// Крестик и «+N ещё» (только пузырь).
    var onDismiss: (() -> Void)?
    var more = 0
    /// Пузырь всплывает сам: пока кнопки не «взведены», клик, который пользователь уже делал по другому, их не нажмёт.
    var armDelay: Duration = .zero
    @State private var armed = false

    var body: some View {
        let truncated = Self.isTruncated(pending.inputPreview)
        let remember = Self.rememberLines(pending.remember)
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 1) {
                    if compact {
                        Text(verbatim: session.displayName)
                            .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                    Text(verbatim: "Просит разрешение: \(UntrustedText.clean(pending.toolName, limit: 60))")
                        .font(.callout.weight(.semibold))
                }
                Spacer(minLength: 0)
                if more > 0 {
                    Text(verbatim: "+\(more) ещё").font(.caption).foregroundStyle(.secondary)
                }
                if let onDismiss {
                    Button(action: onDismiss) { Image(systemName: "xmark") }
                        .buttonStyle(.plain)
                        .help("Скрыть (ответить можно в чате или в IDE)")
                }
            }
            // Команда показывается целиком, с переводами строк и прокруткой: по обрезанному началу
            // нельзя судить, что выполнится. Если превью неполное — «Разрешить» недоступно.
            ScrollView {
                Text(verbatim: UntrustedText.multiline(pending.inputPreview, limit: Self.previewLimit))
                    .font(.system(.caption, design: .monospaced))
                    .textSelection(.enabled)
                    .padding(6)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(height: Self.previewHeight(pending.inputPreview, maxHeight: compact ? 90 : 140))
            .background(Color.black.opacity(0.25), in: RoundedRectangle(cornerRadius: 6))
            if truncated {
                Text(verbatim: "Превью неполное — посмотри полную команду в терминале и ответь там.")
                    .font(.caption).foregroundStyle(.red)
            }
            if let remember {
                ForEach(Array(remember.enumerated()), id: \.offset) { _, line in
                    (Text(verbatim: "Запомнить: ").font(.caption)
                        + Text(verbatim: line.rules).font(.system(.caption, design: .monospaced))
                        + Text(verbatim: " — \(line.place)").font(.caption))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            HStack {
                Button("Разрешить") { answer(allow: true) }
                    .buttonStyle(.borderedProminent)
                    .disabled(truncated)
                if remember != nil {
                    Button("Разрешать всегда") { answer(allow: true, always: true) }
                        .disabled(truncated)
                }
                Button("Отклонить") { answer(allow: false) }
            }
            .disabled(model.permissionBusy || !(armed || armDelay == .zero))
            if let error = model.permissionError(for: pending.requestId) {
                Text(verbatim: UntrustedText.clean(error, limit: 80))
                    .font(.caption).foregroundStyle(.red).lineLimit(2)
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(ChatTheme.approval)
        .overlay(alignment: .leading) { Rectangle().fill(ChatTheme.amber).frame(width: 3) }
        .task(id: pending.requestId) {
            armed = false
            guard armDelay != .zero else { return }
            try? await Task.sleep(for: armDelay)
            armed = !Task.isCancelled
        }
    }

    private func answer(allow: Bool, always: Bool = false) {
        model.answerPermission(sessionId: session.sessionId, requestId: pending.requestId, allow: allow, always: always)
    }

    private static let previewLimit = 4_000

    /// Строки «что запомнится и где». `nil` — кнопки «Разрешать всегда» нет: правил нет или место
    /// сохранения неизвестно (показать честно нельзя).
    private static func rememberLines(_ items: [BridgeSession.RememberRule]) -> [(rules: String, place: String)]? {
        guard !items.isEmpty else { return nil }
        var lines: [(rules: String, place: String)] = []
        for item in items {
            let place: String
            switch item.destination {
            case "localSettings": place = "для этого проекта (только у тебя)"
            case "projectSettings": place = "для этого проекта (для всех, попадёт в .claude/settings.json)"
            case "userSettings": place = "для всех проектов"
            case "session": place = "до конца сессии"
            default: return nil
            }
            let rules = item.rules.map { UntrustedText.clean($0, limit: 300) }.joined(separator: ", ")
            lines.append((rules, place))
        }
        return lines
    }

    /// Поле команды по её длине (грубо: ~52 символа в строке), но не выше `maxHeight` — дальше прокрутка.
    private static func previewHeight(_ preview: String, maxHeight: CGFloat) -> CGFloat {
        let lines = preview.split(separator: "\n", omittingEmptySubsequences: false)
            .reduce(0) { $0 + max(1, ($1.count + 51) / 52) }
        return min(maxHeight, CGFloat(lines) * 15 + 14)
    }

    /// Превью считается неполным, если оно длиннее лимита показа или кончается многоточием
    /// (так обрезанный текст выглядит и на стороне Claude Code).
    private static func isTruncated(_ preview: String) -> Bool {
        preview.count > previewLimit || preview.hasSuffix("…") || preview.hasSuffix("...")
    }
}
