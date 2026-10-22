import AppKit
import SpriteKit

/// Spritesheet в формате WindowPet: Anna.png + anna.json (строка = состояние, кадры слева направо).
@MainActor
final class SpriteSheet {
    private struct Config: Decodable {
        struct State: Decodable {
            let frameMax: Int
            let spriteLine: Int
        }
        let frameSize: Int
        let states: [String: State]
    }

    private let image: CGImage
    private let config: Config
    private var cache: [String: [SKTexture]] = [:]

    init() {
        guard
            let imageURL = Bundle.main.url(forResource: "Anna", withExtension: "png"),
            let configURL = Bundle.main.url(forResource: "anna", withExtension: "json"),
            let image = NSImage(contentsOf: imageURL)?.cgImage(forProposedRect: nil, context: nil, hints: nil),
            let data = try? Data(contentsOf: configURL),
            let config = try? JSONDecoder().decode(Config.self, from: data)
        else {
            fatalError("Anna.png / anna.json не найдены в Contents/Resources — запускай через scripts/build_app.sh")
        }
        self.image = image
        self.config = config
    }

    /// Кадры состояния; пустой массив, если такого состояния в листе нет.
    func textures(for state: String) -> [SKTexture] {
        if let cached = cache[state] { return cached }
        guard let st = config.states[state] else { return [] }
        let size = config.frameSize
        let textures = (0..<st.frameMax).compactMap { i -> SKTexture? in
            // CGImage считает y сверху, как и сетка листа.
            let rect = CGRect(x: i * size, y: (st.spriteLine - 1) * size, width: size, height: size)
            return image.cropping(to: rect).map { SKTexture(cgImage: $0) }
        }
        cache[state] = textures
        return textures
    }
}
