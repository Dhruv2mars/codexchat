use crate::types::{ModelDescriptor, ModelSelection};

pub fn select_models(models: Vec<ModelDescriptor>) -> ModelSelection {
    let primary = models
        .iter()
        .filter(|model| {
            !model.hidden
                && (model.id.starts_with("gpt-")
                    || model.label.to_ascii_uppercase().starts_with("GPT"))
        })
        .cloned()
        .map(|mut model| {
            model.compatible = true;
            model
        })
        .collect::<Vec<_>>();

    if !primary.is_empty() {
        return ModelSelection {
            compatibility_warning: false,
            models: primary,
        };
    }

    let fallback = models
        .into_iter()
        .filter(|model| !model.hidden)
        .filter(|model| {
            model.model_provider.is_none() || model.model_provider.as_deref() == Some("openai")
        })
        .map(|mut model| {
            model.compatible = false;
            model
        })
        .collect::<Vec<_>>();

    ModelSelection {
        compatibility_warning: !fallback.is_empty(),
        models: fallback,
    }
}

#[cfg(test)]
mod tests {
    use crate::types::ModelDescriptor;

    use super::select_models;

    #[test]
    fn prefers_visible_gpt_models() {
        let selected = select_models(vec![
            ModelDescriptor {
                compatible: false,
                default: false,
                hidden: false,
                id: "gpt-5.4".into(),
                label: "GPT-5.4".into(),
                model_provider: Some("openai".into()),
            },
            ModelDescriptor {
                compatible: false,
                default: false,
                hidden: false,
                id: "o3".into(),
                label: "o3".into(),
                model_provider: Some("openai".into()),
            },
        ]);

        assert!(!selected.compatibility_warning);
        assert_eq!(selected.models.len(), 1);
        assert_eq!(selected.models[0].id, "gpt-5.4");
        assert!(selected.models[0].compatible);
    }

    #[test]
    fn falls_back_to_openai_models_when_no_gpt_model_exists() {
        let selected = select_models(vec![
            ModelDescriptor {
                compatible: false,
                default: false,
                hidden: false,
                id: "codex-mini".into(),
                label: "Codex Mini".into(),
                model_provider: Some("openai".into()),
            },
            ModelDescriptor {
                compatible: false,
                default: false,
                hidden: true,
                id: "gpt-hidden".into(),
                label: "GPT Hidden".into(),
                model_provider: Some("openai".into()),
            },
        ]);

        assert!(selected.compatibility_warning);
        assert_eq!(selected.models.len(), 1);
        assert_eq!(selected.models[0].id, "codex-mini");
        assert!(!selected.models[0].compatible);
    }
}
