import AppKit

/// Подключение питомца ко всем проектам — через `pet-bridge hooks install|uninstall|status`
/// (логика и тесты в bridge/crates/pet-bridge/src/install.rs). Меняет ~/.claude/settings.json
/// только после подтверждения пользователя.
@MainActor
enum HooksInstaller {
    enum Status: String {
        case installed, partial
        case notInstalled = "not_installed"
    }

    static func status() -> Status? {
        let (ok, output) = runBridge(["hooks", "status"])
        return ok ? Status(rawValue: output.trimmingCharacters(in: .whitespacesAndNewlines)) : nil
    }

    /// Спрашивает подтверждение, выполняет и показывает результат. Возвращает новый статус.
    static func toggle(current: Status?) -> Status? {
        let installing = current != .installed
        let alert = NSAlert()
        if installing {
            alert.messageText = "Подключить питомца ко всем проектам?"
            alert.informativeText = """
                В ~/.claude/settings.json добавятся hooks, которые сообщают питомцу о сессиях Claude Code \
                в любой папке. Hooks других программ не трогаются. Строка статуса (statusLine) заменится \
                обёрткой, которая передаёт питомцу лимит за 5 часов и вызывает прежнюю команду. \
                Перед изменением файл сохраняется в ~/.claude-pet/backups. Новые сессии Claude \
                подхватят hooks после перезапуска.
                """
            alert.addButton(withTitle: "Подключить")
        } else {
            alert.messageText = "Отключить питомца от всех проектов?"
            alert.informativeText = """
                Из ~/.claude/settings.json уберутся только hooks питомца, прежняя строка статуса \
                вернётся, остальное останется как есть. \
                Перед изменением файл сохраняется в ~/.claude-pet/backups.
                """
            alert.addButton(withTitle: "Отключить")
        }
        alert.addButton(withTitle: "Отмена")
        NSApp.activate()
        guard alert.runModal() == .alertFirstButtonReturn else { return current }

        let (ok, output) = runBridge(["hooks", installing ? "install" : "uninstall"])
        let result = NSAlert()
        result.alertStyle = ok ? .informational : .warning
        result.messageText = ok ? (installing ? "Питомец подключён" : "Питомец отключён") : "Не получилось"
        result.informativeText = output
        result.runModal()
        return status()
    }

    /// `pet-bridge <args>`; мост работает за миллисекунды — запускаем синхронно.
    static func runBridge(_ args: [String]) -> (ok: Bool, output: String) {
        guard let url = Bundle.main.url(forAuxiliaryExecutable: "pet-bridge") else {
            return (false, "pet-bridge не найден в Contents/MacOS — собери через scripts/build_app.sh")
        }
        let process = Process()
        process.executableURL = url
        process.arguments = args
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        do {
            try process.run()
        } catch {
            return (false, "Не удалось запустить pet-bridge: \(error.localizedDescription)")
        }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return (process.terminationStatus == 0, String(decoding: data, as: UTF8.self))
    }
}
