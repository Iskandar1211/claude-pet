import Foundation

/// Сессия Claude Code из снимка `sessions` (bridge/PROTOCOL.md). Поля — ровно по протоколу.
struct BridgeSession: Decodable, Equatable, Identifiable, Sendable {
    struct PendingPermission: Decodable, Equatable, Sendable {
        let requestId: String
        let toolName: String
        /// Недоверенный текст из сессии — показывать только как обычный текст.
        let inputPreview: String

        enum CodingKeys: String, CodingKey {
            case requestId = "request_id"
            case toolName = "tool_name"
            case inputPreview = "input_preview"
        }
    }

    let sessionId: String
    let shortId: String
    let entrypoint: String?
    let cwd: String?
    let status: PetStatus
    /// Недоверенный текст из сессии.
    let lastMessage: String?
    let hasChannel: Bool
    let pendingPermission: PendingPermission?

    var id: String { sessionId }

    enum CodingKeys: String, CodingKey {
        case sessionId = "session_id"
        case shortId = "short_id"
        case entrypoint, cwd, status
        case lastMessage = "last_message"
        case hasChannel = "has_channel"
        case pendingPermission = "pending_permission"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        sessionId = try c.decode(String.self, forKey: .sessionId)
        shortId = try c.decode(String.self, forKey: .shortId)
        entrypoint = try c.decodeIfPresent(String.self, forKey: .entrypoint)
        cwd = try c.decodeIfPresent(String.self, forKey: .cwd)
        // Неизвестный статус из будущей версии моста не должен ронять весь снимок.
        status = PetStatus(rawValue: try c.decode(String.self, forKey: .status)) ?? .idle
        lastMessage = try c.decodeIfPresent(String.self, forKey: .lastMessage)
        hasChannel = try c.decode(Bool.self, forKey: .hasChannel)
        pendingPermission = try c.decodeIfPresent(PendingPermission.self, forKey: .pendingPermission)
    }
}

extension BridgeSession {
    /// Имя папки из `cwd` (недоверенный текст — очищаем), иначе короткий id.
    var displayName: String {
        guard let cwd, let name = cwd.split(separator: "/").last else { return shortId }
        return UntrustedText.clean(String(name), limit: 40)
    }

    /// Откуда сессия; `claude-vscode` — это и VS Code, и Cursor.
    var sourceTitle: String {
        switch entrypoint {
        case "cli": "терминал"
        case "claude-vscode": "VS Code / Cursor"
        case let other?: UntrustedText.clean(other, limit: 20)
        case nil: "неизвестно"
        }
    }
}
