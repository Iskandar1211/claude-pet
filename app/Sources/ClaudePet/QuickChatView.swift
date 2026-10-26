import SwiftUI

/// Содержимое панели Quick Chat. Все тексты из сессий (сообщения, input_preview, last_message, cwd)
/// недоверенные: очищаются `UntrustedText`; ответы Claude — Markdown без ссылок (`MarkdownView`),
/// остальное — `Text(verbatim:)`.
struct QuickChatView: View {
    let model: QuickChatModel
    let onClose: () -> Void
    @FocusState private var inputFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            header
            sessionList
            Divider()
            if let session = model.selected {
                conversation(for: session)
            } else {
                emptyState
            }
        }
        .frame(width: QuickChatPanel.size.width, height: QuickChatPanel.size.height)
        .background(ChatTheme.panelBackground)
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(ChatTheme.panelBorder))
        .environment(\.colorScheme, .dark)
        .tint(ChatTheme.accent)
        .onAppear { inputFocused = true }
        .onChange(of: model.focusTick) { inputFocused = true }
    }

    // MARK: - Сессии

    private var header: some View {
        HStack(spacing: 8) {
            Image(systemName: "pawprint.fill").foregroundStyle(ChatTheme.accent)
            Text(verbatim: "Claude Pet")
                .font(.headline)
                .lineLimit(1)
            Spacer(minLength: 6)
            CloseDot(action: onClose)
        }
        .padding(.horizontal, 16)
        .padding(.top, 13)
        .padding(.bottom, 8)
    }

    /// Список подстраивается под число сессий (до трёх строк, дальше прокрутка), остальное место — чату.
    private static let visibleSessionRows = 3
    private static let sessionRowHeight: CGFloat = 46

    private var sessionList: some View {
        ScrollView {
            VStack(spacing: 2) {
                ForEach(model.sessions) { session in
                    SessionRow(session: session, selected: session.id == model.selectedId)
                        .contentShape(Rectangle())
                        .onTapGesture { model.select(session.id) }
                }
            }
            .padding(.horizontal, 8)
            .padding(.bottom, 6)
        }
        .frame(height: CGFloat(min(model.sessions.count, Self.visibleSessionRows)) * Self.sessionRowHeight + 6)
    }

    private var emptyState: some View {
        VStack(spacing: 6) {
            Spacer()
            Text(verbatim: model.bridgeRunning ? "Нет активных сессий Claude Code" : "Мост не запущен")
                .foregroundStyle(.secondary)
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    // MARK: - Разговор

    private func conversation(for session: BridgeSession) -> some View {
        VStack(spacing: 0) {
            approval(for: session)
            messageList
            Divider()
            inputBar
        }
    }

    @ViewBuilder
    private func approval(for session: BridgeSession) -> some View {
        if let pending = session.pendingPermission, session.hasChannel {
            VStack(alignment: .leading, spacing: 6) {
                Text(verbatim: "Просит разрешение: \(UntrustedText.clean(pending.toolName, limit: 60))")
                    .font(.callout.weight(.semibold))
                // Команда показывается целиком, с переводами строк и прокруткой: по обрезанному началу
                // нельзя судить, что выполнится. Если превью неполное — «Разрешить» недоступно.
                let truncated = Self.isTruncated(pending.inputPreview)
                ScrollView {
                    Text(verbatim: UntrustedText.multiline(pending.inputPreview, limit: Self.previewLimit))
                        .font(.system(.caption, design: .monospaced))
                        .textSelection(.enabled)
                        .padding(6)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(height: Self.previewHeight(pending.inputPreview))
                .background(Color.black.opacity(0.25), in: RoundedRectangle(cornerRadius: 6))
                if truncated {
                    Text(verbatim: "Превью неполное — посмотри полную команду в терминале и ответь там.")
                        .font(.caption).foregroundStyle(.red)
                }
                HStack {
                    Button("Разрешить") { model.answerPermission(requestId: pending.requestId, allow: true) }
                        .buttonStyle(.borderedProminent)
                        .disabled(truncated)
                    Button("Отклонить") { model.answerPermission(requestId: pending.requestId, allow: false) }
                    if let error = model.permissionError {
                        Text(verbatim: UntrustedText.clean(error, limit: 80))
                            .font(.caption).foregroundStyle(.red).lineLimit(2)
                    }
                }
                .disabled(model.permissionBusy)
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(ChatTheme.approval)
            .overlay(alignment: .leading) { Rectangle().fill(ChatTheme.amber).frame(width: 3) }
        } else if session.status == .waiting && !session.hasChannel {
            Text(verbatim: "Ждёт одобрения в IDE")
                .font(.callout)
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(ChatTheme.approval)
        }
    }

    private static let previewLimit = 4_000

    /// Поле команды по её длине (грубо: ~52 символа в строке), но не выше 140 — дальше прокрутка.
    private static func previewHeight(_ preview: String) -> CGFloat {
        let lines = preview.split(separator: "\n", omittingEmptySubsequences: false)
            .reduce(0) { $0 + max(1, ($1.count + 51) / 52) }
        return min(140, CGFloat(lines) * 15 + 14)
    }

    /// Превью считается неполным, если оно длиннее лимита показа или кончается многоточием
    /// (так обрезанный текст выглядит и на стороне Claude Code).
    private static func isTruncated(_ preview: String) -> Bool {
        preview.count > previewLimit || preview.hasSuffix("…") || preview.hasSuffix("...")
    }

    private var messageList: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    ForEach(Array(model.messages.enumerated()), id: \.offset) { index, message in
                        MessageBubble(message: message).id(index)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 14)
            }
            .onChange(of: model.messages) { scrollToEnd(proxy) }
            .onAppear { scrollToEnd(proxy) }
        }
    }

    private func scrollToEnd(_ proxy: ScrollViewProxy) {
        if !model.messages.isEmpty { proxy.scrollTo(model.messages.count - 1, anchor: .bottom) }
    }

    private var inputBar: some View {
        @Bindable var model = model
        return VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .bottom, spacing: 8) {
                TextField("Сообщение Claude…", text: $model.draft, axis: .vertical)
                    .textFieldStyle(.plain)
                    .lineLimit(1...5)
                    .focused($inputFocused)
                    .onSubmit { model.sendDraft() }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 7)
                    .background(ChatTheme.surface, in: RoundedRectangle(cornerRadius: 10))
                    .overlay(RoundedRectangle(cornerRadius: 10).stroke(ChatTheme.surfaceBorder))
                voiceButton
                Button { model.sendDraft() } label: {
                    Image(systemName: "arrow.up.circle.fill").font(.system(size: 24))
                }
                .buttonStyle(.plain)
                .foregroundStyle(canSend ? ChatTheme.accent : Color.secondary.opacity(0.5))
                .disabled(!canSend)
                .help("Отправить (Enter)")
            }
            if let feedback = model.feedback {
                feedbackLabel(feedback)
            }
            switch model.voiceInput.state {
            case .preparing:
                Text(verbatim: "Загружаю распознавание речи (только в первый раз)…").foregroundStyle(.secondary).font(.caption)
            case .unavailable(let reason):
                Text(verbatim: reason).foregroundStyle(.orange).font(.caption).lineLimit(2)
            case .idle, .listening:
                EmptyView()
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
    }

    /// Пока Анна читает ответ — «замолчать»; иначе 🎤: начать или закончить диктовку.
    @ViewBuilder
    private var voiceButton: some View {
        if model.voiceOutput.isSpeaking {
            Button { model.voiceOutput.stop() } label: {
                Image(systemName: "stop.circle.fill").font(.system(size: 24))
            }
            .buttonStyle(.plain)
            .foregroundStyle(ChatTheme.accent)
            .help("Замолчать")
        } else {
            let state = model.voiceInput.state
            Button { model.toggleDictation() } label: {
                Image(systemName: state == .listening ? "mic.circle.fill" : "mic.circle")
                    .font(.system(size: 24))
                    .symbolEffect(.pulse, isActive: state == .listening || state == .preparing)
            }
            .buttonStyle(.plain)
            .foregroundStyle(state == .listening ? Color.red : Color.secondary)
            .disabled({ if case .unavailable = state { true } else { false } }())
            .help(state == .listening ? "Закончить диктовку" : "Надиктовать сообщение")
        }
    }

    private var canSend: Bool {
        !model.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && model.feedback != .sending
    }

    @ViewBuilder
    private func feedbackLabel(_ feedback: QuickChatModel.Feedback) -> some View {
        switch feedback {
        case .sending:
            Text(verbatim: "Отправка…").foregroundStyle(.secondary).font(.caption)
        case .delivered:
            Text(verbatim: "Доставлено").foregroundStyle(.green).font(.caption)
        case .unconfirmed:
            Text(verbatim: "Отправлено, доставка не подтверждена").foregroundStyle(.orange).font(.caption)
        case .failed(let error):
            Text(verbatim: "Ошибка: \(UntrustedText.clean(error, limit: 120))")
                .foregroundStyle(.red).font(.caption).lineLimit(2)
        }
    }
}

private struct SessionRow: View {
    let session: BridgeSession
    let selected: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Circle().fill(session.status.color).frame(width: 9, height: 9).padding(.top, 5)
            VStack(alignment: .leading, spacing: 1) {
                HStack {
                    Text(verbatim: session.displayName).font(.callout.weight(.medium)).lineLimit(1)
                    Spacer(minLength: 4)
                    Text(verbatim: "\(session.status.title) · \(session.sourceTitle)")
                        .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
                if let last = session.lastMessage {
                    Text(verbatim: Self.withoutRole(UntrustedText.clean(last, limit: 100)))
                        .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 5)
        .background(selected ? ChatTheme.selectedRow : .clear, in: RoundedRectangle(cornerRadius: 7))
        .overlay {
            if selected { RoundedRectangle(cornerRadius: 7).stroke(ChatTheme.selectedRowBorder) }
        }
    }

    /// Мост кладёт в last_message служебный префикс роли — в списке он только шумит.
    private static func withoutRole(_ text: String) -> String {
        for prefix in ["assistant: ", "user: "] where text.hasPrefix(prefix) {
            return String(text.dropFirst(prefix.count))
        }
        return text
    }
}

/// Запись чата в виде, как в терминале Claude Code: реплика целиком, `⏺ Bash(…)`, `⎿ результат`.
private struct MessageBubble: View {
    let message: BridgeClient.HistoryMessage

    var body: some View {
        switch message.kind {
        case "tool": toolCall
        case "result": toolResult
        default: textBubble
        }
    }

    @ViewBuilder
    private var textBubble: some View {
        let text = UntrustedText.multiline(message.text ?? "", limit: 20_000)
        if message.role == "user" {
            // Свои сообщения — пузырём справа и без Markdown: в них бывают служебные теги inbox/Channel.
            Text(verbatim: text)
                .font(.callout)
                .textSelection(.enabled)
                .padding(.horizontal, 11)
                .padding(.vertical, 7)
                .background(ChatTheme.userBubble, in: RoundedRectangle(cornerRadius: 12))
                .padding(.leading, 48)
                .frame(maxWidth: .infinity, alignment: .trailing)
        } else {
            // Ответ Claude — как в Claude Code: маркер ⏺ и текст с разметкой, без подложки.
            HStack(alignment: .firstTextBaseline, spacing: 7) {
                Text(verbatim: "⏺").font(.caption).foregroundStyle(ChatTheme.accent)
                MarkdownView(text).font(.callout)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var toolCall: some View {
        let name = UntrustedText.clean(message.name ?? "tool", limit: 60)
        let summary = UntrustedText.clean(message.summary ?? "", limit: 160)
        return HStack(alignment: .firstTextBaseline, spacing: 6) {
            Text(verbatim: "⏺").foregroundStyle(.green)
            (Text(verbatim: name).fontWeight(.semibold)
                + Text(verbatim: summary.isEmpty ? "" : "(\(summary))").foregroundStyle(.secondary))
                .lineLimit(2)
                .truncationMode(.tail)
        }
        .font(.system(.caption, design: .monospaced))
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var toolResult: some View {
        let more = message.moreLines ?? 0
        return HStack(alignment: .firstTextBaseline, spacing: 6) {
            Text(verbatim: "⎿")
            VStack(alignment: .leading, spacing: 1) {
                Text(verbatim: UntrustedText.multiline(message.text ?? "", limit: 1_500))
                    .foregroundStyle(message.isError == true ? Color.red : Color.secondary)
                if more > 0 {
                    Text(verbatim: "… +\(more) строк").foregroundStyle(.tertiary)
                }
            }
        }
        .font(.system(.caption, design: .monospaced))
        .padding(.leading, 14)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Недоверенный текст: без управляющих символов и невидимых форматирующих (в том числе bidi-подмены),
/// пробелы схлопнуты, длина ограничена.
enum UntrustedText {
    static func clean(_ text: String, limit: Int) -> String {
        let scalars = text.unicodeScalars.map { scalar -> Character in
            switch scalar.properties.generalCategory {
            case .control, .format, .lineSeparator, .paragraphSeparator: " "
            default: Character(scalar)
            }
        }
        let flat = String(scalars).split(whereSeparator: \.isWhitespace).joined(separator: " ")
        return flat.count > limit ? String(flat.prefix(limit)) + "…" : flat
    }

    /// Как `clean`, но для сообщения чата: переводы строк и табуляция сохраняются.
    static func multiline(_ text: String, limit: Int) -> String {
        let scalars = text.unicodeScalars.map { scalar -> Character in
            if scalar == "\n" || scalar == "\t" { return Character(scalar) }
            switch scalar.properties.generalCategory {
            case .control, .format, .lineSeparator, .paragraphSeparator: return " "
            default: return Character(scalar)
            }
        }
        let cleaned = String(scalars)
        return cleaned.count > limit ? String(cleaned.prefix(limit)) + "…" : cleaned
    }
}

extension PetStatus {
    var color: Color {
        switch self {
        case .idle: .gray
        case .working: .blue
        case .waiting: .orange
        case .done: .green
        case .error: .red
        }
    }
}
