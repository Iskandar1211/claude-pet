import AppKit

/// Контейнер питомца: координаты сверху вниз — спрайт сверху, капсула лимита сразу под ним.
final class PetChromeView: NSView {
    override var isFlipped: Bool { true }
}
