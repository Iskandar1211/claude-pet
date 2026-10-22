import Foundation

/// Запускает `pet-bridge serve` (Contents/MacOS/pet-bridge) и читает его события — протокол
/// в bridge/PROTOCOL.md. Мост упал → перезапуск с паузой 1 → 2 → 4 … 30 с.
@MainActor
final class BridgeClient {
    enum State: Equatable {
        case notRunning
        case running(sessions: Int)
    }

    /// Новый статус питомца по всем сессиям (приходит только при изменении снимка).
    var onPetStatus: ((PetStatus) -> Void)?
    /// Полный снимок сессий — на каждом событии `sessions`.
    var onSessions: (([BridgeSession]) -> Void)?
    var onStateChange: ((State) -> Void)?
    /// 5-часовой лимит подписки — событие `usage`.
    var onUsage: ((Usage) -> Void)?

    struct Usage: Equatable {
        let fiveHourPct: Double
        let resetsAt: Date
        /// Когда status line последний раз прислала данные.
        let updatedAt: Date
    }

    /// Команда приложения → мосту (bridge/PROTOCOL.md).
    enum Command {
        case send(sessionId: String, text: String)
        case permission(sessionId: String, requestId: String, allow: Bool)
        case history(sessionId: String, limit: Int)
    }

    /// Запись чата из ответа `history` (bridge/PROTOCOL.md): `kind` = text | tool | result.
    /// `text` и `summary` — недоверенный текст из сессии.
    struct HistoryMessage: Decodable, Equatable {
        let role: String
        let kind: String
        let text: String?
        let name: String?
        let summary: String?
        let moreLines: Int?
        let isError: Bool?

        enum CodingKeys: String, CodingKey {
            case role, kind, text, name, summary
            case moreLines = "more_lines"
            case isError = "is_error"
        }
    }

    enum Reply {
        /// `result` с `ok:true`; `route`: channel | inbox, `delivery`: sent | confirmed | unconfirmed (для `send`).
        case result(route: String?, delivery: String?)
        case history([HistoryMessage])
    }

    enum CommandError: Error, Equatable {
        case notRunning
        case timeout
        /// `result` с `ok:false` — текст ошибки моста.
        case rejected(String)
    }

    private static let protocolVersion = 1
    private static let maxRestartDelay: TimeInterval = 30
    private static let commandTimeout: Duration = .seconds(10)

    private var process: Process?
    /// Держим stdin открытым: EOF для моста — сигнал завершиться.
    private var stdin: FileHandle?
    private var buffer = Data()
    private var restartDelay: TimeInterval = 1
    private var stopped = false
    private var nextCommandId = 1
    private var pending: [String: CheckedContinuation<Result<Reply, CommandError>, Never>] = [:]

    private(set) var state: State = .notRunning {
        didSet { if state != oldValue { onStateChange?(state) } }
    }

    func start() {
        stopped = false
        // Запись в закрытый stdin упавшего моста иначе убила бы приложение сигналом SIGPIPE.
        signal(SIGPIPE, SIG_IGN)
        launch()
    }

    func stop() {
        stopped = true
        process?.terminate()
    }

    private func launch() {
        guard let url = Bundle.main.url(forAuxiliaryExecutable: "pet-bridge") else {
            NSLog("ClaudePet: pet-bridge не найден в Contents/MacOS — собери через scripts/build_app.sh")
            return
        }
        let process = Process()
        process.executableURL = url
        process.arguments = ["serve"]
        let input = Pipe(), output = Pipe()
        process.standardInput = input
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice

        output.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            Task { @MainActor in self?.consume(data) }
        }
        process.terminationHandler = { [weak self] proc in
            let status = proc.terminationStatus
            Task { @MainActor in self?.handleExit(status) }
        }

        do {
            try process.run()
        } catch {
            NSLog("ClaudePet: не удалось запустить pet-bridge: \(error)")
            scheduleRestart()
            return
        }
        self.process = process
        stdin = input.fileHandleForWriting
        buffer.removeAll()
    }

    /// Отправляет команду и ждёт ответ с тем же `id` (`result` или `history`), не дольше 10 с.
    func send(_ command: Command) async -> Result<Reply, CommandError> {
        guard let stdin else { return .failure(.notRunning) }
        let id = String(nextCommandId)
        nextCommandId += 1
        guard var data = try? JSONEncoder().encode(WireCommand(id: id, command)) else {
            return .failure(.rejected("не удалось закодировать команду"))
        }
        data.append(UInt8(ascii: "\n"))
        return await withCheckedContinuation { continuation in
            pending[id] = continuation
            do {
                try stdin.write(contentsOf: data)
            } catch {
                finish(id, .failure(.notRunning))
                return
            }
            Task { @MainActor [weak self] in
                try? await Task.sleep(for: Self.commandTimeout)
                self?.finish(id, .failure(.timeout))
            }
        }
    }

    /// Каждое продолжение возобновляется ровно один раз: тот, кто первым достал его из словаря.
    private func finish(_ id: String, _ result: Result<Reply, CommandError>) {
        pending.removeValue(forKey: id)?.resume(returning: result)
    }

    private func handleExit(_ status: Int32) {
        process = nil
        stdin = nil
        for id in Array(pending.keys) { finish(id, .failure(.notRunning)) }
        state = .notRunning
        guard !stopped else { return }
        NSLog("ClaudePet: pet-bridge завершился (код \(status)), перезапуск через \(Int(restartDelay)) с")
        scheduleRestart()
    }

    private func scheduleRestart() {
        let delay = restartDelay
        restartDelay = min(restartDelay * 2, Self.maxRestartDelay)
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            guard let self, !self.stopped, self.process == nil else { return }
            self.launch()
        }
    }

    /// stdout приходит кусками — режем на строки по `\n`.
    private func consume(_ data: Data) {
        guard !data.isEmpty else { return }
        buffer.append(data)
        while let newline = buffer.firstIndex(of: UInt8(ascii: "\n")) {
            let line = buffer.subdata(in: buffer.startIndex..<newline)
            buffer.removeSubrange(buffer.startIndex...newline)
            handle(line)
        }
    }

    private func handle(_ line: Data) {
        guard let message = try? JSONDecoder().decode(Message.self, from: line) else {
            NSLog("ClaudePet: непонятная строка от pet-bridge")
            return
        }
        switch message.type {
        case "hello":
            guard message.protocolVersion == Self.protocolVersion else {
                NSLog("ClaudePet: pet-bridge говорит на протоколе \(message.protocolVersion ?? -1), нужен \(Self.protocolVersion)")
                stop()
                return
            }
            restartDelay = 1
            state = .running(sessions: 0)
        case "sessions":
            state = .running(sessions: message.sessions?.count ?? 0)
            onSessions?(message.sessions ?? [])
            if let raw = message.petStatus, let status = PetStatus(rawValue: raw) {
                onPetStatus?(status)
            }
        case "result":
            guard let id = message.id else { return }
            if message.ok == true {
                finish(id, .success(.result(route: message.route, delivery: message.delivery)))
            } else {
                finish(id, .failure(.rejected(message.error ?? "неизвестная ошибка")))
            }
        case "history":
            guard let id = message.id else { return }
            finish(id, .success(.history(message.messages ?? [])))
        case "usage":
            guard let pct = message.fiveHourPct, let resetsAt = message.resetsAt, let updatedAt = message.updatedAt else { return }
            onUsage?(Usage(
                fiveHourPct: pct,
                resetsAt: Date(timeIntervalSince1970: resetsAt),
                updatedAt: Date(timeIntervalSince1970: updatedAt)
            ))
        default:
            break // reply — необязательный ускоритель, не используется
        }
    }

    /// Команда в проводном виде: поля по протоколу, лишние (nil) не кодируются.
    private struct WireCommand: Encodable {
        let id: String
        let cmd: String
        var sessionId: String
        var text: String?
        var requestId: String?
        var allow: Bool?
        var limit: Int?

        enum CodingKeys: String, CodingKey {
            case id, cmd, text, allow, limit
            case sessionId = "session_id"
            case requestId = "request_id"
        }

        init(id: String, _ command: Command) {
            self.id = id
            switch command {
            case .send(let sessionId, let text):
                (cmd, self.sessionId, self.text) = ("send", sessionId, text)
            case .permission(let sessionId, let requestId, let allow):
                (cmd, self.sessionId, self.requestId, self.allow) = ("permission", sessionId, requestId, allow)
            case .history(let sessionId, let limit):
                (cmd, self.sessionId, self.limit) = ("history", sessionId, limit)
            }
        }
    }

    private struct Message: Decodable {
        let type: String
        let protocolVersion: Int?
        let petStatus: String?
        let sessions: [BridgeSession]?
        // result / history
        let id: String?
        let ok: Bool?
        let route: String?
        let delivery: String?
        let error: String?
        let messages: [HistoryMessage]?
        // usage
        let fiveHourPct: Double?
        let resetsAt: TimeInterval?
        let updatedAt: TimeInterval?

        enum CodingKeys: String, CodingKey {
            case type, sessions, id, ok, route, delivery, error, messages
            case protocolVersion = "protocol"
            case petStatus = "pet_status"
            case fiveHourPct = "five_hour_pct"
            case resetsAt = "resets_at"
            case updatedAt = "updated_at"
        }
    }
}
