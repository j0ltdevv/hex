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
        (FRENCH_COLUMN, "Local") => "Local",
        (FRENCH_COLUMN, "Groq") => "Groq",
        (FRENCH_COLUMN, "Paste Groq key") => "Coller la clé Groq",
        (
            FRENCH_COLUMN,
            "Local is the default and runs Hex's on-device model only. Groq sends dictation audio to Groq and requires an API key.",
        ) => {
            "Local est le mode par défaut et n'utilise que le modèle Hex sur l'appareil. Groq envoie l'audio de la dictée à Groq et nécessite une clé API."
        }
        (FRENCH_COLUMN, "Used only when Dictation provider is Groq and a key is set.") => {
            "Utilisé uniquement lorsque le fournisseur de dictée est Groq et qu'une clé est définie."
        }
        (PORTUGUESE_COLUMN, "Local") => "Local",
        (PORTUGUESE_COLUMN, "Groq") => "Groq",
        (PORTUGUESE_COLUMN, "Paste Groq key") => "Colar chave do Groq",
        (
            PORTUGUESE_COLUMN,
            "Local is the default and runs Hex's on-device model only. Groq sends dictation audio to Groq and requires an API key.",
        ) => {
            "Local é o padrão e usa apenas o modelo do Hex no dispositivo. O Groq envia o áudio da ditada ao Groq e exige uma chave de API."
        }
        (PORTUGUESE_COLUMN, "Used only when Dictation provider is Groq and a key is set.") => {
            "Usado apenas quando o provedor de ditado é Groq e uma chave está definida."
        }
        (RUSSIAN_COLUMN, "Local") => "Локальный",
        (RUSSIAN_COLUMN, "Groq") => "Groq",
        (RUSSIAN_COLUMN, "Paste Groq key") => "Вставить ключ Groq",
        (
            RUSSIAN_COLUMN,
            "Local is the default and runs Hex's on-device model only. Groq sends dictation audio to Groq and requires an API key.",
        ) => {
            "Локальный режим используется по умолчанию и задействует только модель Hex на устройстве. Groq отправляет аудио диктовки в Groq и требует ключ API."
        }
        (RUSSIAN_COLUMN, "Used only when Dictation provider is Groq and a key is set.") => {
            "Используется только когда поставщик диктовки — Groq и задан ключ."
        }
        (HUNGARIAN_COLUMN, "Local") => "Helyi",
        (HUNGARIAN_COLUMN, "Groq") => "Groq",
        (HUNGARIAN_COLUMN, "Paste Groq key") => "Groq-kulcs beillesztése",
        (
            HUNGARIAN_COLUMN,
            "Local is the default and runs Hex's on-device model only. Groq sends dictation audio to Groq and requires an API key.",
        ) => {
            "A helyi az alapértelmezett, és csak a Hex eszközön futó modelljét használja. A Groq a diktálás hangját a Groqnak küldi, és API-kulcsot igényel."
        }
        (HUNGARIAN_COLUMN, "Used only when Dictation provider is Groq and a key is set.") => {
            "Csak akkor használatos, ha a diktálás szolgáltatója Groq, és be van állítva kulcs."
        }
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
            "Local",
            "Groq",
            "Paste Groq key",
            "Local is the default and runs Hex's on-device model only. Groq sends dictation audio to Groq and requires an API key.",
            "Used only when Dictation provider is Groq and a key is set.",
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
