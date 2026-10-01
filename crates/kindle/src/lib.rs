use bookreplay_core::Clipping;

pub fn from_kindle_title(value: &str) -> (&str, Vec<&str>) {
    let Some((title, authors)) = value.rsplit_once(" (") else {
        return (value, Vec::new());
    };
    let Some(authors) = authors.strip_suffix(')') else {
        return (value, Vec::new());
    };

    let label = authors.trim().to_lowercase();
    if label.ends_with("edition") || label.ends_with("édition") || label == "kindle" {
        return (value, Vec::new());
    }

    let authors = authors
        .split(';')
        .map(str::trim)
        .filter(|author| !author.is_empty() && *author != "Unknown")
        .collect();

    (title, authors)
}

pub fn parse_clippings(file: &str) -> Vec<Clipping> {
    file.replace("\r\n", "\n")
        .split("==========")
        .filter_map(|entry| {
            let mut parts = entry.trim_start_matches(['\n', '\u{feff}']).splitn(3, '\n');
            let book = parts.next()?.trim_matches('\u{feff}').trim();
            let metadata = parts.next()?.trim();
            let content = parts.next()?.trim();

            (!book.is_empty() && !metadata.is_empty()).then(|| Clipping {
                book: book.into(),
                metadata: metadata.into(),
                content: content.into(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use bookreplay_core::Clipping;

    use super::{from_kindle_title, parse_clippings};

    #[test]
    fn kindle_title_extracts_title_and_authors() {
        assert_eq!(
            from_kindle_title(
                "Head First Design Patterns, 2nd Edition (Eric Freeman;Elisabeth Robson)"
            ),
            (
                "Head First Design Patterns, 2nd Edition",
                vec!["Eric Freeman", "Elisabeth Robson"]
            )
        );
    }

    #[test]
    fn kindle_title_keeps_parenthesized_edition_in_title() {
        assert_eq!(
            from_kindle_title(
                "L'Art de faire les choses jusqu'au bout (French Edition) (Tran, Kevin)"
            ),
            (
                "L'Art de faire les choses jusqu'au bout (French Edition)",
                vec!["Tran, Kevin"]
            )
        );
    }

    #[test]
    fn edition_without_author_stays_in_title() {
        assert_eq!(
            from_kindle_title("Réussir (French Edition)"),
            ("Réussir (French Edition)", vec![])
        );
    }

    #[test]
    fn kindle_title_ignores_unknown_author() {
        assert_eq!(
            from_kindle_title("The Alchemist by Paulo Coelho (Unknown)"),
            ("The Alchemist by Paulo Coelho", Vec::new())
        );
    }

    #[test]
    fn parses_kindles_crlf_format_and_bom() {
        let file =
            "\u{feff}Book (Author)\r\n- Highlight at 1-2\r\n\r\nA useful quote.\r\n==========\r\n";

        assert_eq!(
            parse_clippings(file),
            vec![Clipping {
                book: "Book (Author)".into(),
                metadata: "- Highlight at 1-2".into(),
                content: "A useful quote.".into(),
            }]
        );
    }

    #[test]
    fn parses_every_entry_in_a_synthetic_export() {
        let file = include_str!("fixtures/clippings.txt");
        let clippings = parse_clippings(file);

        assert_eq!(clippings.len(), file.matches("==========").count());
        assert_eq!(clippings[1].content, "A note with\ntwo lines.");
        assert_eq!(clippings[2].content, "");
        assert_eq!(
            from_kindle_title(&clippings[3].book),
            (
                "Livre inventé (French Edition)",
                vec!["Auteur, Un", "Autre, Une"]
            )
        );
    }
}
