//! General › Advanced › Dictation's Groq cloud transcription settings.

use rune::{El, TextEdit};
use rune_kit::TextInput;

use super::super::controls::{CardRow, Choice, Paint, select_sized, setting_row};
use super::super::settings::toggle_menu;
use super::super::{Menu, Ui};
use crate::app_settings::AppSettings;
use crate::desktop::i18n::tr;
use crate::groq::{GroqApiKey, GroqModel, SpeechProvider};

pub(in crate::rune_ui) fn draft(key: &Option<GroqApiKey>) -> TextEdit {
    TextEdit::new(crate::groq::draft_text(key))
}

pub(in crate::rune_ui) fn set_draft(ui: &mut Ui, key: &Option<GroqApiKey>) {
    ui.groq_api_key = draft(key);
}

pub(in crate::rune_ui) fn project(ui: &Ui, candidate: &mut AppSettings) {
    candidate.groq_api_key =
        crate::groq::draft_key(&ui.groq_api_key.text, ui.settings.groq_api_key.is_some());
}

pub(in crate::rune_ui) fn differs(ui: &Ui, key: &Option<GroqApiKey>) -> bool {
    crate::groq::draft_differs(key, &ui.groq_api_key.text)
}

const KEY_WIDTH: f32 = 280.0;
const MODEL_WIDTH: f32 = 220.0;
const PROVIDER_WIDTH: f32 = 160.0;

pub(super) fn rows(ui: &Ui, p: Paint, layer: &mut Vec<El>) -> Vec<CardRow> {
    let provider = &ui.settings.speech_provider;
    let provider_select = select_sized(
        p,
        "speech-provider",
        tr("Dictation provider"),
        provider_label(provider),
        None,
        ui.open == Some(Menu::Pick("speech-provider")),
        vec![
            Choice {
                label: tr("Local").to_owned(),
                detail: None,
                selected: provider == &SpeechProvider::Local,
                choose: Box::new(|ui| {
                    ui.open = None;
                    ui.update(|settings| settings.speech_provider = SpeechProvider::Local);
                }),
            },
            Choice {
                label: tr("Groq").to_owned(),
                detail: None,
                selected: provider == &SpeechProvider::Groq,
                choose: Box::new(|ui| {
                    ui.open = None;
                    ui.update(|settings| settings.speech_provider = SpeechProvider::Groq);
                }),
            },
        ],
        |ui| toggle_menu(ui, Menu::Pick("speech-provider")),
        layer,
        PROVIDER_WIDTH,
    );
    let mut rows = Vec::<CardRow>::new();
    rows.push(
        setting_row(
            p,
            "speech-provider",
            tr("Dictation provider"),
            Some(
                tr("Local is the default and runs Hex's on-device model only. Groq sends dictation audio to Groq and requires an API key."),
            ),
            provider_select,
        )
        .into(),
    );
    // Local keeps its base card: the key and model belong to the cloud provider.
    if cloud_rows_visible(provider) {
        let key = TextInput::new(&ui.groq_api_key)
            .placeholder(tr("Paste Groq key"))
            .label(tr("Groq API key"))
            .width(Some(KEY_WIDTH))
            .secure()
            .revealed(false)
            .on_edit(|ui: &mut Ui| &mut ui.groq_api_key)
            .build(p.k)
            .key("groq-api-key-input");
        let current = ui.settings.groq_model;
        let model = select_sized(
            p,
            "groq-model",
            tr("Groq model"),
            groq_model_label(current),
            None,
            ui.open == Some(Menu::Pick("groq-model")),
            vec![
                Choice {
                    label: tr("Whisper Large V3").to_owned(),
                    detail: None,
                    selected: current == GroqModel::LargeV3,
                    choose: Box::new(|ui| {
                        ui.open = None;
                        ui.update(|settings| settings.groq_model = GroqModel::LargeV3);
                    }),
                },
                Choice {
                    label: tr("Whisper Large V3 Turbo").to_owned(),
                    detail: None,
                    selected: current == GroqModel::Turbo,
                    choose: Box::new(|ui| {
                        ui.open = None;
                        ui.update(|settings| settings.groq_model = GroqModel::Turbo);
                    }),
                },
            ],
            |ui| toggle_menu(ui, Menu::Pick("groq-model")),
            layer,
            MODEL_WIDTH,
        );
        rows.push(setting_row(p, "groq-api-key", tr("Groq API key"), None, key).into());
        rows.push(
            setting_row(
                p,
                "groq-model",
                tr("Groq model"),
                Some(tr(
                    "Used only when Dictation provider is Groq and a key is set.",
                )),
                model,
            )
            .into(),
        );
    }
    rows
}

fn provider_label(provider: &SpeechProvider) -> &'static str {
    match provider {
        SpeechProvider::Local => tr("Local"),
        SpeechProvider::Groq => tr("Groq"),
    }
}

fn groq_model_label(model: GroqModel) -> &'static str {
    match model {
        GroqModel::LargeV3 => tr("Whisper Large V3"),
        GroqModel::Turbo => tr("Whisper Large V3 Turbo"),
    }
}

/// The key and model settings apply to Groq only.
fn cloud_rows_visible(provider: &SpeechProvider) -> bool {
    matches!(provider, SpeechProvider::Groq)
}

#[cfg(test)]
mod tests {
    use super::{cloud_rows_visible, groq_model_label, provider_label};
    use crate::desktop::i18n::apply;
    use crate::groq::{GroqModel, SpeechProvider};

    #[test]
    fn local_hides_the_cloud_only_rows() {
        assert!(!cloud_rows_visible(&SpeechProvider::Local));
        assert!(cloud_rows_visible(&SpeechProvider::Groq));
    }

    #[test]
    fn provider_and_model_labels_follow_the_interface_language() {
        apply(Some("en"));
        assert_eq!(provider_label(&SpeechProvider::Local), "Local");
        assert_eq!(groq_model_label(GroqModel::LargeV3), "Whisper Large V3");
        apply(Some("de"));
        assert_eq!(provider_label(&SpeechProvider::Local), "Lokal");
        assert_eq!(provider_label(&SpeechProvider::Groq), "Groq");
        assert_eq!(groq_model_label(GroqModel::Turbo), "Whisper Large V3 Turbo");
        apply(Some("en"));
    }
}
