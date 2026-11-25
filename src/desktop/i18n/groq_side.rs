//! Provider strings for the four side tables that already sit at the 1,500-line
//! ceiling. The main table keeps the complete key set; these side-only entries
//! live here so adding a provider does not force unrelated table churn.

use super::{FRENCH_COLUMN, HUNGARIAN_COLUMN, PORTUGUESE_COLUMN, RUSSIAN_COLUMN};

pub(super) fn translate(column: u8, english: &str) -> Option<&'static str> {
    let text = match (column, english) {
        (FRENCH_COLUMN, "Groq API key") => "Clé API Groq",
        (FRENCH_COLUMN, "Groq model") => "Modèle Groq",
        (FRENCH_COLUMN, "Whisper Large V3") => "Whisper Large V3",
        (FRENCH_COLUMN, "Whisper Large V3 Turbo") => "Whisper Large V3 Turbo",
        (FRENCH_COLUMN, "Dictation provider") => "Fournisseur de dictée",
        (PORTUGUESE_COLUMN, "Groq API key") => "Chave de API do Groq",
        (PORTUGUESE_COLUMN, "Groq model") => "Modelo do Groq",
        (PORTUGUESE_COLUMN, "Whisper Large V3") => "Whisper Large V3",
        (PORTUGUESE_COLUMN, "Whisper Large V3 Turbo") => "Whisper Large V3 Turbo",
        (PORTUGUESE_COLUMN, "Dictation provider") => "Provedor de ditado",
        (RUSSIAN_COLUMN, "Groq API key") => "Ключ API Groq",
        (RUSSIAN_COLUMN, "Groq model") => "Модель Groq",
        (RUSSIAN_COLUMN, "Whisper Large V3") => "Whisper Large V3",
        (RUSSIAN_COLUMN, "Whisper Large V3 Turbo") => "Whisper Large V3 Turbo",
        (RUSSIAN_COLUMN, "Dictation provider") => "Поставщик диктовки",
        (HUNGARIAN_COLUMN, "Groq API key") => "Groq API-kulcs",
        (HUNGARIAN_COLUMN, "Groq model") => "Groq-modell",
        (HUNGARIAN_COLUMN, "Whisper Large V3") => "Whisper Large V3",
        (HUNGARIAN_COLUMN, "Whisper Large V3 Turbo") => "Whisper Large V3 Turbo",
        (HUNGARIAN_COLUMN, "Dictation provider") => "Diktálás szolgáltatója",
        _ => return None,
    };
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_side_language_has_every_supplemental_provider_string() {
        for english in [
            "Groq API key",
            "Groq model",
            "Whisper Large V3",
            "Whisper Large V3 Turbo",
            "Dictation provider",
        ] {
            for column in [
                FRENCH_COLUMN,
                PORTUGUESE_COLUMN,
                RUSSIAN_COLUMN,
                HUNGARIAN_COLUMN,
            ] {
                assert!(
                    translate(column, english).is_some(),
                    "column {column} lacks {english}"
                );
            }
        }
    }
}
