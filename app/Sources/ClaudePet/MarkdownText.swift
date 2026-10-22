import SwiftUI

/// Блок Markdown-ответа. SwiftUI `Text` умеет только строчную разметку (жирный, курсив, `код`),
/// поэтому блоки — заголовки, списки, код, цитаты — режем сами, а строчную отдаём `AttributedString`.
enum MarkdownBlock: Equatable {
    case paragraph(String)
    case heading(level: Int, String)
    case listItem(marker: String, indent: Int, String)
    case code(String)
    case quote(String)
    case rule
}

enum Markdown {
    /// Чистая функция: текст → блоки. Незакрытый блок кода идёт до конца текста.
    static func blocks(_ text: String) -> [MarkdownBlock] {
        var blocks: [MarkdownBlock] = []
        var paragraph: [String] = []
        var code: [String]?

        func flush() {
            if !paragraph.isEmpty { blocks.append(.paragraph(paragraph.joined(separator: "\n"))) }
            paragraph.removeAll()
        }

        for line in text.components(separatedBy: "\n") {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("```") {
                if let lines = code {
                    blocks.append(.code(lines.joined(separator: "\n")))
                    code = nil
                } else {
                    flush()
                    code = []
                }
                continue
            }
            if code != nil {
                code?.append(line)
                continue
            }
            if trimmed.isEmpty {
                flush()
            } else if let heading = heading(trimmed) {
                flush()
                blocks.append(heading)
            } else if trimmed == "---" || trimmed == "***" || trimmed == "___" {
                flush()
                blocks.append(.rule)
            } else if trimmed.hasPrefix(">") {
                flush()
                blocks.append(.quote(String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces)))
            } else if let item = listItem(line) {
                flush()
                blocks.append(item)
            } else {
                paragraph.append(trimmed)
            }
        }
        if let lines = code { blocks.append(.code(lines.joined(separator: "\n"))) }
        flush()
        return blocks
    }

    private static func heading(_ line: String) -> MarkdownBlock? {
        let hashes = line.prefix { $0 == "#" }.count
        guard (1...6).contains(hashes), line.dropFirst(hashes).first == " " else { return nil }
        return .heading(level: hashes, String(line.dropFirst(hashes + 1)))
    }

    /// `- пункт`, `* пункт`, `1. пункт`, `2) пункт`; отступ — по два пробела на уровень.
    private static func listItem(_ line: String) -> MarkdownBlock? {
        let indent = line.prefix { $0 == " " }.count / 2
        let rest = line.drop { $0 == " " }
        if let first = rest.first, "-*+".contains(first), rest.dropFirst().first == " " {
            return .listItem(marker: "•", indent: indent, String(rest.dropFirst(2)))
        }
        let digits = rest.prefix { $0.isNumber }
        let after = rest.dropFirst(digits.count)
        if !digits.isEmpty, digits.count <= 3, let dot = after.first, ".)".contains(dot), after.dropFirst().first == " " {
            return .listItem(marker: "\(digits).", indent: indent, String(after.dropFirst(2)))
        }
        return nil
    }

    /// Строчная разметка. Текст недоверенный: ссылки и картинки снимаем — видно текст, но не кликается.
    static func inline(_ text: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(
            interpretedSyntax: .inlineOnlyPreservingWhitespace,
            failurePolicy: .returnPartiallyParsedIfPossible
        )
        var result = (try? AttributedString(markdown: text, options: options)) ?? AttributedString(text)
        for run in result.runs where run.link != nil || run.imageURL != nil {
            result[run.range].link = nil
            result[run.range].imageURL = nil
        }
        return result
    }
}

/// Ответ Claude с Markdown. Текст уже очищен `UntrustedText.multiline`.
struct MarkdownView: View {
    let blocks: [MarkdownBlock]

    init(_ text: String) {
        blocks = Markdown.blocks(text)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                view(for: block)
            }
        }
        .textSelection(.enabled)
    }

    @ViewBuilder
    private func view(for block: MarkdownBlock) -> some View {
        switch block {
        case .paragraph(let text):
            Text(Markdown.inline(text))
        case .heading(let level, let text):
            Text(Markdown.inline(text))
                .font(level == 1 ? .headline : .subheadline.weight(.semibold))
                .padding(.top, 2)
        case .listItem(let marker, let indent, let text):
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(verbatim: marker).foregroundStyle(.secondary)
                Text(Markdown.inline(text))
            }
            .padding(.leading, CGFloat(indent) * 14)
        case .code(let code):
            ScrollView(.horizontal, showsIndicators: false) {
                Text(verbatim: code)
                    .font(.system(.caption, design: .monospaced))
                    .fixedSize(horizontal: true, vertical: false)
                    .padding(8)
            }
            .background(ChatTheme.surface, in: RoundedRectangle(cornerRadius: 7))
        case .quote(let text):
            Text(Markdown.inline(text))
                .foregroundStyle(.secondary)
                .padding(.leading, 9)
                .overlay(alignment: .leading) {
                    RoundedRectangle(cornerRadius: 1).fill(.tertiary).frame(width: 2)
                }
        case .rule:
            Divider()
        }
    }
}
