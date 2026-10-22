import Foundation

/// Читает `~/.claude-pet/usage.json` (пишет statusLine) и дополняет снимком сессий:
/// VS Code/Cursor не вызывают status line, поэтому файл часто устаревает, а в `last_message` уже «session limit».
enum UsageStore {
    /// Свежий `usage.json` из терминала важнее эвристики по сессиям.
    private static let fileFreshFor: TimeInterval = 3 * 60

    private static let path = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".claude-pet/usage.json")

    static func load() -> BridgeClient.Usage? {
        parse(try? Data(contentsOf: path))
    }

    /// Что показать на капсуле: файл, если свежий; иначе 100% при исчерпанном лимите в сессии; иначе устаревший файл.
    static func effective(sessions: [BridgeSession]) -> BridgeClient.Usage? {
        let now = Date()
        let fromFile = load()
        let fileFresh = fromFile.map { now.timeIntervalSince($0.updatedAt) <= fileFreshFor } ?? false

        if let fromFile, fileFresh, fromFile.resetsAt > now {
            return fromFile
        }

        if sessionLimitExhausted(sessions) {
            return BridgeClient.Usage(
                fiveHourPct: 100,
                resetsAt: fromFile?.resetsAt ?? now.addingTimeInterval(5 * 3600),
                updatedAt: now
            )
        }

        guard let fromFile, fromFile.resetsAt > now else { return nil }
        return fromFile
    }

    private static func parse(_ data: Data?) -> BridgeClient.Usage? {
        guard let data,
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let five = (json["rate_limits"] as? [String: Any])?["five_hour"] as? [String: Any]
        else { return nil }

        guard let pct = double(five["used_percentage"]),
              let resets = seconds(five["resets_at"]),
              let updated = seconds(json["updated_at"])
        else { return nil }

        return BridgeClient.Usage(
            fiveHourPct: pct,
            resetsAt: Date(timeIntervalSince1970: resets),
            updatedAt: Date(timeIntervalSince1970: updated)
        )
    }

    private static func sessionLimitExhausted(_ sessions: [BridgeSession]) -> Bool {
        sessions.contains { session in
            guard let message = session.lastMessage?.lowercased() else { return false }
            return message.contains("session limit")
                || message.contains("rate limit")
                || message.contains("usage limit")
                || message.contains("лимит сессии")
                || message.contains("лимит использования")
        }
    }

    /// «только что», «3 мин назад», «2 ч назад».
    static func ageText(from date: Date, now: Date = Date()) -> String {
        let seconds = Int(now.timeIntervalSince(date))
        switch seconds {
        case ..<60: return "только что"
        case ..<3600: return "\(seconds / 60) мин назад"
        default: return "\(seconds / 3600) ч назад"
        }
    }

    /// Строка для меню лапки: что известно о лимите прямо сейчас.
    static func describe(_ usage: BridgeClient.Usage?, now: Date = Date()) -> String {
        guard let usage else { return "Лимит: нет данных — запусти claude в терминале" }
        guard usage.resetsAt > now else { return "Лимит: окно сброшено — отправь запрос в терминале" }
        return "Лимит за 5 ч: \(Int(usage.fiveHourPct.rounded()))% · \(ageText(from: usage.updatedAt, now: now))"
    }

    private static func double(_ value: Any?) -> Double? {
        switch value {
        case let n as Double: n
        case let n as Int: Double(n)
        case let n as NSNumber: n.doubleValue
        default: nil
        }
    }

    private static func seconds(_ value: Any?) -> TimeInterval? {
        double(value)
    }
}
