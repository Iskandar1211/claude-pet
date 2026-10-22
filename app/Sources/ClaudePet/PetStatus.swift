/// Статус сессий Claude Code; rawValue = имя строки в anna.json.
/// Приоритет при нескольких сессиях (как у ChatGPT Pets) считает мост: waiting → error → done → working.
enum PetStatus: String, CaseIterable {
    case idle, working, waiting, done, error

    var title: String {
        switch self {
        case .idle: "Ожидание"
        case .working: "Работает"
        case .waiting: "Ждёт одобрения"
        case .done: "Готово"
        case .error: "Ошибка"
        }
    }
}
