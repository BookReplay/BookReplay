use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    OpenLibrary,
    GoogleBooks,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BookCandidate {
    pub provider: Provider,
    pub provider_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub cover_url: Option<String>,
    pub first_publish_year: Option<i32>,
    pub edition_count: Option<i32>,
    pub isbns: Vec<String>,
}

impl BookCandidate {
    pub fn open_library_key(&self) -> Option<&str> {
        (self.provider == Provider::OpenLibrary).then_some(self.provider_id.as_str())
    }

    pub fn google_books_volume_id(&self) -> Option<&str> {
        (self.provider == Provider::GoogleBooks).then_some(self.provider_id.as_str())
    }

    pub fn validate(&self) -> bool {
        let valid_id = match self.provider {
            Provider::OpenLibrary => self
                .provider_id
                .strip_prefix("/works/OL")
                .and_then(|id| id.strip_suffix('W'))
                .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())),
            Provider::GoogleBooks => {
                !self.provider_id.is_empty()
                    && self.provider_id.len() <= 128
                    && self
                        .provider_id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            }
        };
        valid_id
            && !self.title.trim().is_empty()
            && self.cover_url.as_deref().is_none_or(valid_cover_url)
            && (self.provider != Provider::GoogleBooks || self.first_publish_year.is_none())
    }
}

// Both hosts are allowed because ISBN-matched candidates can supplement covers.
pub fn valid_cover_url(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
        && match url.host_str() {
            Some("covers.openlibrary.org") => {
                url.path()
                    .strip_prefix("/b/id/")
                    .and_then(|path| path.strip_suffix("-L.jpg"))
                    .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
                    && url.query().is_none()
            }
            Some("books.google.com" | "books.googleusercontent.com") => {
                url.path() == "/books/content"
                    && url.query_pairs().all(|(key, _)| {
                        matches!(
                            key.as_ref(),
                            "id" | "printsec"
                                | "img"
                                | "zoom"
                                | "edge"
                                | "source"
                                | "gbs_api"
                                | "w"
                                | "h"
                                | "sig"
                        )
                    })
            }
            _ => false,
        }
}

pub(crate) fn clean_title(value: &str) -> &str {
    let mut title = value.trim();
    loop {
        let suffix = title
            .rfind(['(', '[', ',', '—'])
            .map(|at| (at, &title[at + 1..]));
        let Some((at, suffix)) = suffix else { break };
        let label = suffix.trim_end_matches([')', ']']).trim().to_lowercase();
        if !(label.ends_with("edition") || label.ends_with("édition") || label == "kindle") {
            break;
        }
        title = title[..at].trim();
    }
    title
}

pub(crate) fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn author_name(value: &str) -> String {
    match value.split_once(',') {
        Some((last, first)) => normalize(&format!("{first} {last}")),
        None => normalize(value),
    }
}

fn similarity(left: &str, right: &str) -> u32 {
    if left == right && !left.is_empty() {
        return 10_000;
    }
    let a: std::collections::BTreeSet<_> = left.split_whitespace().collect();
    let b: std::collections::BTreeSet<_> = right.split_whitespace().collect();
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    (20_000 * a.intersection(&b).count() / (a.len() + b.len())) as u32
}

pub(crate) fn rank(books: &mut [BookCandidate], title: &str, authors: &[&str]) {
    let title = normalize(clean_title(title));
    let score = |book: &BookCandidate| {
        let author = authors
            .iter()
            .flat_map(|a| {
                book.authors
                    .iter()
                    .map(move |b| similarity(&author_name(a), &author_name(b)))
            })
            .max()
            .unwrap_or(0);
        (
            similarity(&title, &normalize(clean_title(&book.title))),
            author,
            usize::from(book.cover_url.is_some())
                + usize::from(!book.authors.is_empty())
                + usize::from(book.first_publish_year.is_some())
                + usize::from(!book.isbns.is_empty()),
            book.provider == Provider::OpenLibrary,
        )
    };
    books.sort_by(|a, b| {
        score(b)
            .cmp(&score(a))
            .then_with(|| a.provider_id.cmp(&b.provider_id))
    });
}

pub(crate) fn needs_more(books: &[BookCandidate], title: &str) -> bool {
    books.first().is_none_or(|book| {
        normalize(clean_title(&book.title)) != normalize(clean_title(title))
            || book.authors.is_empty()
            || book.cover_url.is_none()
    })
}

fn normalized_isbn(value: &str) -> Option<String> {
    let value: String = value
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(char::to_uppercase)
        .collect();
    let valid = (value.len() == 13 && value.bytes().all(|b| b.is_ascii_digit()))
        || (value.len() == 10
            && value.as_bytes()[..9].iter().all(|b| b.is_ascii_digit())
            && value.as_bytes()[9].is_ascii_alphanumeric()
            && (value.as_bytes()[9].is_ascii_digit() || value.ends_with('X')));
    valid.then_some(value)
}

pub(crate) fn supplement(books: &[BookCandidate]) -> Option<BookCandidate> {
    let mut best = books.first()?.clone();
    for other in &books[1..] {
        if !best
            .isbns
            .iter()
            .filter_map(|isbn| normalized_isbn(isbn))
            .any(|isbn| {
                other
                    .isbns
                    .iter()
                    .filter_map(|other| normalized_isbn(other))
                    .any(|other| isbn == other)
            })
        {
            continue;
        }
        if best.authors.is_empty() {
            best.authors.clone_from(&other.authors);
        }
        if best.cover_url.is_none() {
            best.cover_url.clone_from(&other.cover_url);
        }
    }
    Some(best)
}

#[derive(Deserialize)]
pub(crate) struct GoogleResponse {
    #[serde(rename = "totalItems")]
    pub total_items: u64,
    #[serde(default)]
    pub items: Vec<serde_json::Value>,
}
#[derive(Deserialize)]
pub(crate) struct GoogleVolume {
    id: String,
    #[serde(rename = "volumeInfo")]
    info: GoogleInfo,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleInfo {
    title: String,
    subtitle: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    image_links: Option<GoogleImages>,
    #[serde(default)]
    industry_identifiers: Vec<GoogleIdentifier>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleImages {
    thumbnail: Option<String>,
    small_thumbnail: Option<String>,
}
#[derive(Deserialize)]
struct GoogleIdentifier {
    #[serde(rename = "type")]
    kind: String,
    identifier: String,
}

impl From<GoogleVolume> for BookCandidate {
    fn from(volume: GoogleVolume) -> Self {
        let info = volume.info;
        let cover_url = info
            .image_links
            .and_then(|images| images.thumbnail.or(images.small_thumbnail))
            .map(|url| url.replacen("http://", "https://", 1))
            .filter(|url| valid_cover_url(url));
        Self {
            provider: Provider::GoogleBooks,
            provider_id: volume.id,
            title: info
                .subtitle
                .filter(|s| !s.trim().is_empty())
                .map_or_else(|| info.title.clone(), |s| format!("{}: {s}", info.title)),
            authors: info
                .authors
                .into_iter()
                .filter(|s| !s.trim().is_empty())
                .collect(),
            cover_url,
            first_publish_year: None,
            edition_count: None,
            isbns: info
                .industry_identifiers
                .into_iter()
                .filter(|id| matches!(id.kind.as_str(), "ISBN_10" | "ISBN_13"))
                .map(|id| id.identifier)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(provider: Provider, id: &str, title: &str, author: &str) -> BookCandidate {
        BookCandidate {
            provider,
            provider_id: id.into(),
            title: title.into(),
            authors: if author.is_empty() {
                vec![]
            } else {
                vec![author.into()]
            },
            cover_url: None,
            first_publish_year: None,
            edition_count: None,
            isbns: vec![],
        }
    }

    #[test]
    fn normalizes_edition_punctuation_accents_and_reversed_authors() {
        assert_eq!(
            normalize(clean_title("  L’Art: Réussir! (French Edition) ")),
            "l art réussir"
        );
        assert_eq!(clean_title("Patterns, 2nd Edition"), "Patterns");
        assert_eq!(author_name("Tran, Kevin"), author_name("Kevin Tran"));
    }

    #[test]
    fn ranking_prefers_title_then_author_then_completeness_then_source() {
        let exact = book(
            Provider::OpenLibrary,
            "/works/OL1W",
            "Réussir",
            "Tran, Kevin",
        );
        let mut wrong_author = exact.clone();
        wrong_author.provider_id = "/works/OL2W".into();
        wrong_author.authors = vec!["Someone Else".into()];
        wrong_author.cover_url = Some("https://covers.openlibrary.org/b/id/1-L.jpg".into());
        let mut translated = wrong_author.clone();
        translated.title = "Succeed".into();
        let mut google = exact.clone();
        google.provider = Provider::GoogleBooks;
        google.provider_id = "abc".into();
        let mut books = vec![translated, wrong_author, google.clone(), exact.clone()];
        rank(&mut books, "Réussir (French Edition)", &["Kevin Tran"]);
        assert_eq!(books[0], exact);
        google.cover_url = Some("https://books.google.com/books/content?id=abc&img=1".into());
        books.push(google.clone());
        rank(&mut books, "Réussir", &["Kevin Tran"]);
        assert_eq!(books[0], google);
        let expected = books.clone();
        books.reverse();
        rank(&mut books, "Réussir", &["Kevin Tran"]);
        assert_eq!(books, expected);
    }

    #[test]
    fn supplement_requires_shared_isbn() {
        let mut first = book(Provider::OpenLibrary, "/works/OL1W", "Book", "");
        first.isbns = vec!["9781234567890".into()];
        let mut other = book(Provider::GoogleBooks, "abc", "Book", "Author");
        other.cover_url = Some("https://books.google.com/books/content?id=abc&img=1".into());
        assert_eq!(
            supplement(&[first.clone(), other.clone()]),
            Some(first.clone())
        );
        other.isbns = first.isbns.clone();
        let filled = supplement(&[first, other]).unwrap();
        assert_eq!(filled.authors, ["Author"]);
        assert!(filled.cover_url.is_some());
    }

    #[test]
    fn rejects_unsafe_covers_identifiers_and_google_first_publication_year() {
        for url in [
            "https://books.google.com.evil.test/books/content",
            "http://books.google.com/books/content",
            "https://user@books.google.com/books/content",
            "https://books.google.com/books/content?key=secret",
            "https://example.org/cover.jpg",
        ] {
            assert!(!valid_cover_url(url), "{url}");
        }
        let mut candidate = book(Provider::GoogleBooks, "abc_-12", "Book", "");
        assert!(candidate.validate());
        candidate.provider_id = "../abc".into();
        assert!(!candidate.validate());
        candidate.provider_id = "abc".into();
        candidate.first_publish_year = Some(2020);
        assert!(!candidate.validate());
    }
}
