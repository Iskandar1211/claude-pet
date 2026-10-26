import Foundation
import Observation

/// Состояние Quick Chat: сессии из снимков моста, история выбранной сессии, отправка и одобрения.
/// Вся сеть — через `BridgeClient.send`; здесь только решения «что показать».
@MainActor
@Observable
final class QuickChatModel {
    enum Feedback: Equatable {
        case sending
        case delivered
        case unconfirmed
        case failed(String)
    }

    private(set) var sessions: [BridgeSession] = []
    private(set) var messages: [BridgeClient.HistoryMessage] = []
    private(set) var feedback: Feedback?
    private(set) var permissionBusy = false
    private(set) var permissionError: String?
    private(set) var bridgeRunning = false
    /// Растёт при каждом открытии панели — view по нему возвращает фокус в поле ввода.
    private(set) var focusTick = 0
    var draft = ""
    private(set) var selectedId: String?

    private static let historyLimit = 60
    private let bridge: BridgeClient
    private var isOpen = false
    private var historyTask: Task<Void, Never>?
    /// Что показано в истории: при изменении статуса/последнего сообщения выбранной сессии — перезапрос.
    private var historyKey: String?

    let voiceInput = VoiceInput()
    let voiceOutput = VoiceOutput()
    /// Черновик до начала диктовки: распознанный текст дописывается к нему.
    private var draftBeforeDictation = ""
    /// В черновике есть надиктованное — ответ на такое сообщение будет прочитан вслух.
    private var draftDictated = false
    /// Голосовое сообщение, ответ на которое ждём: прочитать, когда сессия поработает и закончит.
    private var awaitingReply: (sessionId: String, text: String, sawWorking: Bool)?

    init(bridge: BridgeClient) {
        self.bridge = bridge
        voiceInput.onTranscript = { [weak self] text in self?.dictated(text) }
    }

    var selected: BridgeSession? {
        sessions.first { $0.id == selectedId }
    }

    // MARK: - События

    func update(sessions newSessions: [BridgeSession]) {
        sessions = newSessions
        if selected == nil {
            // Сессия, которая ждёт одобрения, важнее остальных.
            selectedId = (newSessions.first { $0.pendingPermission != nil } ?? newSessions.first)?.id
            resetConversation()
        }
        if isOpen { reloadHistoryIfNeeded() }
        checkAwaitedReply()
    }

    func bridgeStateChanged(_ state: BridgeClient.State) {
        switch state {
        case .notRunning:
            bridgeRunning = false
            update(sessions: [])
        case .running:
            bridgeRunning = true
        }
    }

    func panelOpened() {
        isOpen = true
        focusTick += 1
        reloadHistoryIfNeeded(force: true)
    }

    func panelClosed() {
        isOpen = false
    }

    /// Панель открыта именно на этой сессии (для уведомлений: тогда пользователь и так смотрит на неё).
    func isShowing(_ sessionId: String) -> Bool {
        isOpen && selectedId == sessionId
    }

    func select(_ id: String) {
        guard id != selectedId else { return }
        selectedId = id
        resetConversation()
        reloadHistoryIfNeeded(force: true)
    }

    // MARK: - История

    private func resetConversation() {
        historyTask?.cancel()
        messages = []
        feedback = nil
        permissionError = nil
        historyKey = nil
    }

    private func reloadHistoryIfNeeded(force: Bool = false) {
        guard let session = selected else { return }
        let key = "\(session.id)|\(session.status.rawValue)|\(session.lastMessage ?? "")"
        guard force || key != historyKey else { return }
        historyKey = key
        historyTask?.cancel()
        historyTask = Task { [bridge] in
            let result = await bridge.send(.history(sessionId: session.id, limit: Self.historyLimit))
            guard !Task.isCancelled, selectedId == session.id else { return }
            if case .success(.history(let list)) = result {
                messages = list
            }
            // Ошибка истории не критична: остаётся прежний список, следующее изменение снимка повторит запрос.
        }
    }

    // MARK: - Отправка

    func sendDraft() {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, let session = selected, feedback != .sending else { return }
        voiceInput.stop()
        voiceOutput.stop()
        let byVoice = draftDictated
        feedback = .sending
        Task {
            let result = await bridge.send(.send(sessionId: session.id, text: text))
            guard selectedId == session.id else { return }
            switch result {
            case .success(.result(_, let delivery)):
                feedback = delivery == "unconfirmed" ? .unconfirmed : .delivered
                draft = ""
                draftDictated = false
                if byVoice { awaitingReply = (session.id, text, false) }
            case .success:
                feedback = .failed("неожиданный ответ моста")
            case .failure(let error):
                feedback = .failed(error.message)
            }
        }
    }

    // MARK: - Голос

    /// 🎤 в поле ввода: начать или закончить диктовку. Говорящая Анна при этом замолкает.
    func toggleDictation() {
        if voiceInput.state == .idle {
            voiceOutput.stop()
            draftBeforeDictation = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        voiceInput.toggle()
    }

    private func dictated(_ text: String) {
        // Поздние результаты после «стоп» или отправки не должны вернуть текст в очищенное поле.
        guard voiceInput.state == .listening || voiceInput.state == .preparing, !text.isEmpty else { return }
        draft = draftBeforeDictation.isEmpty ? text : "\(draftBeforeDictation) \(text)"
        draftDictated = true
    }

    /// Ответ на голосовое сообщение читается, когда сессия сначала поработала, а потом закончила:
    /// сразу после отправки в снимке ещё может стоять «готово» от прошлого ответа.
    private func checkAwaitedReply() {
        guard let awaited = awaitingReply else { return }
        guard let session = sessions.first(where: { $0.id == awaited.sessionId }) else {
            awaitingReply = nil
            return
        }
        switch session.status {
        case .working, .waiting:
            awaitingReply?.sawWorking = true
        case .done where awaited.sawWorking:
            awaitingReply = nil
            Task { await speakReply(sessionId: awaited.sessionId, to: awaited.text) }
        case .error, .idle:
            if awaited.sawWorking { awaitingReply = nil }
        case .done:
            break
        }
    }

    private func speakReply(sessionId: String, to sent: String) async {
        let result = await bridge.send(.history(sessionId: sessionId, limit: Self.historyLimit))
        guard case .success(.history(let list)) = result else { return }
        let reply = Self.reply(in: list, to: sent)
        voiceOutput.speak(UntrustedText.multiline(reply, limit: Self.spokenLimit))
    }

    /// Дольше этого Анна не читает: длинные ответы всё равно видны в чате.
    private static let spokenLimit = 4_000

    /// Чистая функция: текст ответов ассистента после нашего сообщения и до следующего сообщения
    /// пользователя (ищем по началу текста, как мост подтверждает доставку); не нашли — после последнего.
    static func reply(in messages: [BridgeClient.HistoryMessage], to sent: String) -> String {
        let prefix = String(sent.prefix(40))
        let userTexts = messages.indices.filter { messages[$0].role == "user" && messages[$0].kind == "text" }
        let start = userTexts.last { messages[$0].text?.contains(prefix) == true } ?? userTexts.last ?? -1
        let end = userTexts.first { $0 > start } ?? messages.endIndex
        return messages[(start + 1)..<end]
            .filter { $0.role == "assistant" && $0.kind == "text" }
            .compactMap(\.text)
            .joined(separator: "\n\n")
    }

    // MARK: - Одобрения

    /// `requestId` берём из карточки, которую видел пользователь: одобряем именно показанный запрос,
    /// а не тот, что оказался «текущим» к моменту клика.
    func answerPermission(requestId: String, allow: Bool) {
        guard let session = selected, !permissionBusy else { return }
        permissionBusy = true
        permissionError = nil
        Task {
            let result = await bridge.send(.permission(sessionId: session.id, requestId: requestId, allow: allow))
            permissionBusy = false
            if case .failure(let error) = result {
                permissionError = error.message
            }
            // При успехе карточка исчезнет сама: в следующем снимке pending_permission уже нет.
        }
    }
}

extension BridgeClient.CommandError {
    var message: String {
        switch self {
        case .notRunning: "мост не запущен"
        case .timeout: "нет ответа от моста"
        case .rejected(let text): text
        }
    }
}
