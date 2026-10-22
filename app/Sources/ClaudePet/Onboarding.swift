import AppKit
import SwiftUI

/// Окно первого запуска: подключить питомца к Claude Code и (по желанию) поставить команду `claude-pet`.
/// Кнопки в окне и есть согласие пользователя — текст рядом говорит, что именно изменится.
@MainActor
@Observable
final class OnboardingModel {
    private(set) var hooks = HooksInstaller.status()
    private(set) var launcherInstalled = OnboardingModel.launcherStatus()
    private(set) var message: String?
    private(set) var failed = false
    /// Сообщить меню лапки, что статус подключения поменялся.
    var onHooksChanged: ((HooksInstaller.Status?) -> Void)?

    func connect() {
        let (ok, output) = HooksInstaller.runBridge(["hooks", "install"])
        hooks = HooksInstaller.status()
        report(ok, ok ? "Подключено. Новые сессии Claude увидят питомца после перезапуска." : output)
        onHooksChanged?(hooks)
    }

    func installLauncher() {
        guard let channel = Bundle.main.url(forAuxiliaryExecutable: "pet-channel") else {
            report(false, "Channel не собран: установи Bun (https://bun.sh) и запусти ./install.sh ещё раз.")
            return
        }
        let (ok, output) = HooksInstaller.runBridge(["launcher", "install", channel.path])
        launcherInstalled = Self.launcherStatus()
        report(ok, ok
            ? "Готово: запускай `claude-pet` вместо `claude`. Если команда не находится, добавь в ~/.zshrc строку export PATH=\"$HOME/.local/bin:$PATH\""
            : output)
    }

    private func report(_ ok: Bool, _ text: String) {
        failed = !ok
        message = text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func launcherStatus() -> Bool {
        HooksInstaller.runBridge(["launcher", "status"]).output.hasPrefix("installed")
    }
}

struct OnboardingView: View {
    let model: OnboardingModel
    let close: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 10) {
                Image(systemName: "pawprint.fill").font(.system(size: 28)).foregroundStyle(.orange)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Claude Pet").font(.title2.weight(.semibold))
                    Text("Анна показывает, что делают твои сессии Claude Code: работает, ждёт одобрения, готово.")
                        .foregroundStyle(.secondary)
                }
            }

            step(
                done: model.hooks == .installed,
                title: "Подключить к Claude Code",
                detail: """
                    В ~/.claude/settings.json добавятся hooks питомца (hooks других программ не трогаются), \
                    строка статуса получит обёртку для лимита за 5 часов. Перед изменением файл сохраняется \
                    в ~/.claude-pet/backups; отключить можно из меню лапки.
                    """,
                button: "Подключить",
                action: model.connect
            )

            step(
                done: model.launcherInstalled,
                title: "Команда claude-pet (по желанию)",
                detail: """
                    Запуск Claude Code с Channel питомца: тогда из Quick Chat можно отвечать сессии и \
                    одобрять действия. Ставится в ~/.local/bin/claude-pet; обычный `claude` не меняется.
                    """,
                button: "Установить",
                action: model.installLauncher
            )

            if let message = model.message {
                Text(verbatim: message)
                    .font(.callout)
                    .foregroundStyle(model.failed ? Color.red : Color.secondary)
                    .textSelection(.enabled)
            }

            HStack {
                Spacer()
                Button(model.hooks == .installed ? "Готово" : "Позже", action: close)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(22)
        .frame(width: 460)
    }

    private func step(done: Bool, title: String, detail: String, button: String, action: @escaping () -> Void) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: done ? "checkmark.circle.fill" : "circle")
                .font(.title3)
                .foregroundStyle(done ? Color.green : Color.secondary)
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.headline)
                Text(detail).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 8)
            Button(done ? "Готово" : button, action: action).disabled(done)
        }
    }
}

/// Показывает окно приветствия: само при первом запуске (если ещё не подключено) и из меню лапки.
@MainActor
final class OnboardingController {
    private static let shownKey = "onboardingShown"
    private var window: NSWindow?
    let model = OnboardingModel()

    func showOnFirstLaunch() {
        guard !UserDefaults.standard.bool(forKey: Self.shownKey), model.hooks != .installed else { return }
        show()
    }

    func show() {
        UserDefaults.standard.set(true, forKey: Self.shownKey)
        if window == nil {
            let window = NSWindow(
                contentRect: .zero,
                styleMask: [.titled, .closable],
                backing: .buffered,
                defer: false
            )
            window.title = "Claude Pet"
            window.isReleasedWhenClosed = false
            window.contentView = NSHostingView(rootView: OnboardingView(model: model) { [weak window] in window?.close() })
            window.center()
            self.window = window
        }
        NSApp.activate()
        window?.makeKeyAndOrderFront(nil)
    }
}
