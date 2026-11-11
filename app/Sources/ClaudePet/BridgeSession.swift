import Foundation

/// Сессия Claude Code из снимка `sessions` (bridge/PROTOCOL.md). Поля — ровно по протоколу.
struct BridgeSession: Decodable, Equatable, Identifiable, Sendable {
    struct PendingPermission: Decodable, Equatable, Sendable {
        let requestId: String
        let toolName: String
        /// Недоверенный текст из сессии — показывать только как обычный текст.
        let inputPreview: String
        /// Что запомнит «Разрешать всегда»; пусто — кнопки нет (Channel, старый мост, нет предложений Claude Code).
        let remember: [RememberRule]

        enum CodingKeys: String, CodingKey {
            case requestId = "request_id"
            case toolName = "tool_name"
            case inputPreview = "input_preview"
            case remember
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            requestId = try c.decode(String.self, forKey: .requestId)
            toolName = try c.decode(String.self, forKey: .toolName)
            inputPreview = try c.decode(String.self, forKey: .inputPreview)
            remember = try c.decodeIfPresent([RememberRule].self, forKey: .remember) ?? []
        }
    }

    /// Правка прав, которую предложил Claude Code. Недоверенный текст — показывать только через `UntrustedText`.
    struct RememberRule: Decodable, Equatable, Sendable {
        /// `Tool(ruleContent)` или `Tool`.
        let rules: [String]
        /// `session` | `localSettings` | `projectSettings` | `userSettings`; другое значение — не показываем кнопку.
        let destination: String
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
    /// Модель, effort и режим из транскрипта — недоверенный текст; nil, пока мост их не увидел.
    let modelName: String?
    let effort: String?
    let permissionMode: String?
    /// Сколько субагентов сейчас работает.
    let subagents: Int

    var id: String { sessionId }

    enum CodingKeys: String, CodingKey {
        case sessionId = "session_id"
        case shortId = "short_id"
        case entrypoint, cwd, status
        case lastMessage = "last_message"
        case hasChannel = "has_channel"
        case pendingPermission = "pending_permission"
        case modelName = "model_name"
        case effort, subagents
        case permissionMode = "permission_mode"
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
        modelName = try c.decodeIfPresent(String.self, forKey: .modelName)
        effort = try c.decodeIfPresent(String.self, forKey: .effort)
        permissionMode = try c.decodeIfPresent(String.self, forKey: .permissionMode)
        subagents = try c.decodeIfPresent(Int.self, forKey: .subagents) ?? 0
    }
}

extension BridgeSession {
    /// Имя папки из `cwd` (недоверенный текст — очищаем), иначе короткий id.
    var displayName: String {
        guard let cwd, let name = cwd.split(separator: "/").last else { return shortId }
        return UntrustedText.clean(String(name), limit: 40)
    }

    /// Строка состояния, как в панели Claude Code: «Fable 5.2 · Medium · Auto · 2 agents»; nil — показывать нечего.
    var statusLine: String? {
        var parts: [String] = []
        if let modelName { parts.append(UntrustedText.clean(modelName, limit: 30)) }
        if let effort { parts.append(UntrustedText.clean(effort, limit: 12).capitalized) }
        if let permissionMode {
            // Названия режимов — как в переключателе расширения.
            switch permissionMode {
            case "default": parts.append("Manual")
            case "acceptEdits": parts.append("Edit automatically")
            case "plan": parts.append("Plan")
            case "auto": parts.append("Auto")
            case "bypassPermissions": parts.append("Bypass permissions")
            default: parts.append(UntrustedText.clean(permissionMode, limit: 20))
            }
        }
        if subagents > 0 { parts.append(subagents == 1 ? "1 agent" : "\(subagents) agents") }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
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
