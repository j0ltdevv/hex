//! General › Advanced › Dictation's Groq cloud transcription settings.

use rune::{El, TextEdit};
use rune_kit::TextInput;

use super::super::controls::{Choice, Paint, select_sized, setting_row};
use super::super::settings::toggle_menu;
use super::super::{Menu, Ui};
use crate::app_settings::AppSettings;
use crate::desktop::i18n::tr;
use crate::groq::{GroqApiKey, GroqModel};

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

pub(super) fn rows(ui: &Ui, p: Paint, layer: &mut Vec<El>) -> Vec<super::super::controls::CardRow> {
    let key = TextInput::new(&ui.groq_api_key)
        .placeholder("Paste Groq key")
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
        tr(groq_model_label(current)),
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
    vec![
        setting_row(p, "groq-api-key", tr("Groq API key"), None, key).into(),
        setting_row(
            p,
            "groq-model",
            tr("Groq model"),
            Some(tr(
                "Uses Groq when a key is set; otherwise Hex uses the local model.",
            )),
            model,
        )
        .into(),
    ]
}

fn groq_model_label(model: GroqModel) -> &'static str {
    match model {
        GroqModel::LargeV3 => "Whisper Large V3",
        GroqModel::Turbo => "Whisper Large V3 Turbo",
    }
}
