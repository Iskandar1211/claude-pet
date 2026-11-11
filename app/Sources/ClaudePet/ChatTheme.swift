import SwiftUI

/// Палитра Quick Chat — цвета темы VS Code «Dark Modern» (как панель Claude Code в редакторе),
/// плюс оранжевый акцент Claude. Панель всегда тёмная; все цвета чата берутся отсюда.
enum ChatTheme {
    static let accent = Color(red: 1.0, green: 0.59, blue: 0.24)
    /// Карточка одобрения — янтарная, чтобы не сливаться с оранжевым акцентом.
    static let amber = Color(red: 1.0, green: 0.78, blue: 0.25)
    static let approval = amber.opacity(0.15)

    /// Основной текст (`foreground` темы); второстепенный — системный `.secondary` от него.
    static let text = hex(0xCCCCCC)
    /// Блоки кода (`textCodeBlock.background`).
    static let surface = hex(0x2B2B2B)
    /// Рамка поля ввода и своих сообщений (`input.border`).
    static let surfaceBorder = hex(0x3C3C3C)
    /// Поле ввода (`input.background`).
    static let inputBackground = hex(0x313131)
    /// Нейтральные плашки без оранжевой заливки: оранжевый на тёмном при малой прозрачности даёт грязно-коричневый.
    static let selectedRow = Color.white.opacity(0.08)
    static let selectedRowBorder = accent.opacity(0.45)
    static let userBubble = hex(0x313131)
    static let panelBorder = hex(0x2B2B2B)

    /// Текст сообщений и поля ввода; инструменты, результаты и код — моноширинным чуть мельче.
    static let messageSize: CGFloat = 14
    static let messageFont = Font.system(size: messageSize)
    static let codeFont = Font.system(size: 11, design: .monospaced)

    /// Фон боковой панели (`sideBar.background`), где в редакторе живёт Claude Code.
    static let panelBackground = hex(0x181818)

    private static func hex(_ rgb: UInt32) -> Color {
        Color(red: Double(rgb >> 16 & 0xFF) / 255, green: Double(rgb >> 8 & 0xFF) / 255, blue: Double(rgb & 0xFF) / 255)
    }
}

/// Красная точка закрытия в правом верхнем углу, как у системных окон macOS (только закрытие).
struct CloseDot: View {
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Circle()
                .fill(Color(red: 1.0, green: 0.37, blue: 0.34))
                .frame(width: 13, height: 13)
                .overlay {
                    if hovering {
                        Image(systemName: "xmark")
                            .font(.system(size: 7, weight: .heavy))
                            .foregroundStyle(Color.black.opacity(0.6))
                    }
                }
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .help("Закрыть (Esc)")
        .accessibilityLabel("Закрыть")
    }
}
