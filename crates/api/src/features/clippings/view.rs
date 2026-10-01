use serde::Serialize;

#[derive(Serialize)]
pub struct ImportResponse {
    message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parsed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inserted: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duplicates: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    books_inserted: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    books_queued: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preview: Option<Box<str>>,
}

impl ImportResponse {
    pub fn success(
        parsed: usize,
        inserted: usize,
        books_inserted: usize,
        preview: Option<Box<str>>,
    ) -> Self {
        Self {
            message: "clippings imported",
            parsed: Some(parsed),
            inserted: Some(inserted),
            duplicates: Some(parsed - inserted),
            books_inserted: Some(books_inserted),
            books_queued: Some(books_inserted),
            preview,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::ImportResponse;

    #[test]
    fn success_reports_new_books_as_queued() {
        let response = serde_json::to_value(ImportResponse::success(
            5,
            3,
            2,
            Some("A useful quote.".into()),
        ))
        .expect("response should serialize");

        assert_eq!(
            response,
            json!({
                "message": "clippings imported",
                "parsed": 5,
                "inserted": 3,
                "duplicates": 2,
                "books_inserted": 2,
                "books_queued": 2,
                "preview": "A useful quote."
            })
        );
    }
}
