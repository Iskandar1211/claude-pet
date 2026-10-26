import SwiftUI

/// Тёплое свечение над полем ввода, как в голосовом режиме Claude: пока ты диктуешь, оно следует
/// за громкостью микрофона; пока Анна читает ответ — медленно «дышит».
/// Цвет — акцент Анны, но сложением света (`.plusLighter`): обычное наложение полупрозрачного
/// оранжевого на сине-серый фон дало бы бурое пятно (см. ChatTheme).
struct VoiceGlow: View {
    enum Mode: Equatable {
        case listening(level: Float)
        case speaking
    }

    let mode: Mode

    /// Цикл «дыхания» при чтении ответа.
    private static let breath: TimeInterval = 2.5

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30, paused: mode != .speaking)) { context in
            let strength = strength(at: context.date)
            ZStack {
                EllipticalGradient(
                    colors: [ChatTheme.accent.opacity(0.6), ChatTheme.accent.opacity(0.16), .clear],
                    center: .bottom,
                    startRadiusFraction: 0,
                    endRadiusFraction: 0.5
                )
                Grain()
                    .mask(EllipticalGradient(colors: [.white, .clear], center: .bottom, endRadiusFraction: 0.45))
            }
            .opacity(0.35 + 0.65 * strength)
            .scaleEffect(x: 0.85 + 0.15 * strength, y: 0.75 + 0.25 * strength, anchor: .bottom)
        }
        .frame(height: 280)
        .blendMode(.plusLighter)
        .allowsHitTesting(false)
        .animation(.easeOut(duration: 0.15), value: mode)
    }

    /// 0…1: громкость при диктовке, синусоида при чтении ответа.
    private func strength(at date: Date) -> CGFloat {
        switch mode {
        case .listening(let level):
            CGFloat(level)
        case .speaking:
            // Не до нуля: при чтении свечение должно быть видно всё время, меняется только яркость.
            0.75 + 0.25 * sin(date.timeIntervalSinceReferenceDate * 2 * .pi / Self.breath)
        }
    }
}

/// Мелкое неподвижное «зерно» свечения. Без входных данных — SwiftUI рисует его один раз, а не каждый кадр.
private struct Grain: View {
    var body: some View {
        Canvas { context, size in
            var random = SplitMix(seed: 7) // одинаковый узор при каждом открытии
            for _ in 0..<Int(size.width * size.height / 90) {
                let point = CGPoint(x: random.next() * size.width, y: random.next() * size.height)
                context.fill(
                    Path(ellipseIn: CGRect(origin: point, size: CGSize(width: 1, height: 1))),
                    with: .color(.white.opacity(0.05 + 0.12 * random.next()))
                )
            }
        }
    }

    /// Детерминированный генератор: `SystemRandomNumberGenerator` нельзя засеять.
    private struct SplitMix {
        var state: UInt64
        init(seed: UInt64) { state = seed }

        mutating func next() -> CGFloat {
            state &+= 0x9E37_79B9_7F4A_7C15
            var z = state
            z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
            z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
            return CGFloat((z ^ (z >> 31)) >> 11) / CGFloat(1 << 53)
        }
    }
}

/// Плашка «••• Стоп» / «••• Готово» вместо круглой кнопки, пока идёт голос; точки мигают по очереди.
struct VoicePill: View {
    let title: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                TimelineView(.periodic(from: .now, by: 0.3)) { context in
                    let step = Int(context.date.timeIntervalSinceReferenceDate / 0.3) % 3
                    HStack(spacing: 3) {
                        ForEach(0..<3) { index in
                            Circle()
                                .frame(width: 4, height: 4)
                                .opacity(index == step ? 1 : 0.35)
                        }
                    }
                }
                Text(verbatim: title).font(.callout.weight(.medium))
            }
            .foregroundStyle(ChatTheme.accent)
            .padding(.horizontal, 10)
            .frame(height: 28)
            // Радиус чуть меньше половины высоты: обводка полной капсулы рисуется с хвостами на прямых участках.
            .background(Color.white.opacity(0.10), in: RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(ChatTheme.selectedRowBorder))
        }
        .buttonStyle(.plain)
    }
}
