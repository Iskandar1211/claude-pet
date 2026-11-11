import AppKit

/// Вложения чата: файлы к сообщению передаются путями в тексте (Claude читает их сам), вставленные из буфера
/// картинки сохраняются в ~/.claude-pet/attachments.
enum Attachments {
    static let limit = 10

    private static let directory = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".claude-pet/attachments")
    private static let pastePrefix = "paste-"

    /// Чистая функция: текст пользователя + пути → итоговое сообщение. Без текста — один блок со списком.
    static func message(text: String, paths: [String]) -> String {
        guard !paths.isEmpty else { return text }
        let block = "Прикреплённые файлы:\n" + paths.map { "- \($0)" }.joined(separator: "\n")
        return text.isEmpty ? block : "\(text)\n\n\(block)"
    }

    /// Файлы из буфера (скопированы в Finder) или, если их нет и в буфере нет текста, картинка, сохранённая как PNG.
    /// Пусто — в буфере текст или нечто иное: вставку делает обычное поле ввода.
    static func pasteboardFiles(_ pasteboard: NSPasteboard) -> [URL] {
        let files = pasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL]
        if let files, !files.isEmpty { return files }
        guard pasteboard.string(forType: .string) == nil, let url = savePastedImage(pasteboard) else { return [] }
        return [url]
    }

    private static func savePastedImage(_ pasteboard: NSPasteboard) -> URL? {
        let png = pasteboard.data(forType: .png)
            ?? pasteboard.data(forType: .tiff)
                .flatMap { NSBitmapImageRep(data: $0)?.representation(using: .png, properties: [:]) }
        guard let png else { return nil }
        let stamp = DateFormatter()
        stamp.locale = Locale(identifier: "en_US_POSIX")
        stamp.dateFormat = "yyyyMMdd-HHmmss"
        let tail = UUID().uuidString.prefix(6).lowercased()
        let url = directory.appendingPathComponent("\(pastePrefix)\(stamp.string(from: Date()))-\(tail).png")
        do {
            try FileManager.default.createDirectory(
                at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
            guard FileManager.default.createFile(atPath: url.path, contents: png, attributes: [.posixPermissions: 0o600])
            else { throw CocoaError(.fileWriteUnknown) }
            return url
        } catch {
            NSLog("ClaudePet: не удалось сохранить вставленную картинку: \(error)")
            return nil
        }
    }

    /// При запуске: paste-*.png старше суток удаляются; в каталоге больше ничего не трогаем.
    static func removeStalePastes() {
        let cutoff = Date().addingTimeInterval(-24 * 3600)
        let files = (try? FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: [.contentModificationDateKey])) ?? []
        for url in files where url.lastPathComponent.hasPrefix(pastePrefix) && url.pathExtension == "png" {
            let modified = try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate
            if let modified, modified < cutoff { try? FileManager.default.removeItem(at: url) }
        }
    }
}
