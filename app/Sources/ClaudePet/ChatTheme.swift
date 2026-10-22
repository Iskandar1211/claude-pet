import SwiftUI

/// Палитра Quick Chat: тёмная панель в цветах иконки приложения (сине-серый фон, оранжевый акцент Анны).
/// Панель всегда тёмная — как терминал Claude Code; все цвета чата берутся отсюда.
enum ChatTheme {
    static let accent = Color(red: 1.0, green: 0.59, blue: 0.24)
    /// Карточка одобрения — янтарная, чтобы не сливаться с оранжевым акцентом.
    static let amber = Color(red: 1.0, green: 0.78, blue: 0.25)
    static let approval = amber.opacity(0.15)

    static let surface = Color.white.opacity(0.07)
    static let surfaceBorder = Color.white.opacity(0.13)
    /// Нейтральные плашки без оранжевой заливки: оранжевый на тёмном при малой прозрачности даёт грязно-коричневый.
    static let selectedRow = Color.white.opacity(0.10)
    static let selectedRowBorder = accent.opacity(0.45)
    static let userBubble = Color.white.opacity(0.13)
    static let panelBorder = Color.white.opacity(0.10)

    static let panelBackground = LinearGradient(
        colors: [Color(red: 0.20, green: 0.23, blue: 0.31), Color(red: 0.09, green: 0.11, blue: 0.15)],
        startPoint: .top,
        endPoint: .bottom
    )
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
