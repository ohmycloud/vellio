//! Response identity is separate from the requested profile/"default" alias.
use super::Provider;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct ModelIdentity {
    pub model: Option<String>,
    pub provider: Option<Provider>,
}
impl ModelIdentity {
    pub fn new(model: &str, provider: Option<Provider>) -> Self {
        Self {
            model: concrete_model(model).map(str::to_owned),
            provider,
        }
    }
}
pub struct ModelAnswer {
    pub text: String,
    pub identity: ModelIdentity,
}
pub(super) fn concrete_model(model: &str) -> Option<&str> {
    let model = model.trim();
    (!model.is_empty()
        && !model.eq_ignore_ascii_case("default")
        && !model.eq_ignore_ascii_case("auto"))
    .then_some(model)
}
/// Legacy answers cannot be attributed to today's default profile: it may have
/// changed since the answer was generated. Never invent historical provenance.
pub fn answer_model_label(entry: &Value) -> String {
    let model = entry["model"]
        .as_str()
        .and_then(concrete_model)
        .or_else(|| entry["by"].as_str().and_then(concrete_model));
    let provider = serde_json::from_value::<Provider>(entry["provider"].clone()).ok();
    match (model, provider) {
        (Some(model), Some(provider)) => format!("{model} · {}", provider.label()),
        (None, Some(provider)) => format!("模型未报告 · {}", provider.label()),
        (Some(model), None) => format!("{model} · 类型未记录"),
        (None, None) => "模型与类型未记录".into(),
    }
}
pub(super) fn claude_reported_model(value: &Value) -> Option<String> {
    value["model"]
        .as_str()
        .and_then(concrete_model)
        .map(str::to_owned)
        .or_else(|| {
            let usage = value["modelUsage"].as_object()?;
            // Multiple models cannot be attributed to a single primary model safely.
            (usage.len() == 1)
                .then(|| {
                    usage
                        .keys()
                        .next()
                        .and_then(|s| concrete_model(s))
                        .map(str::to_owned)
                })
                .flatten()
        })
}
pub(super) fn codex_reported_model(diagnostics: &str) -> Option<String> {
    // Codex's startup metadata, not generated answer prose. --color never keeps
    // this header plain. Stop at the prompt separator to avoid reading quoted
    // conversation text as provider metadata.
    for line in diagnostics.lines() {
        if line.trim() == "user" {
            break;
        }
        if let Some(model) = line.trim().strip_prefix("model:").and_then(concrete_model) {
            return Some(model.to_owned());
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn labels_use_response_identity_and_do_not_guess_legacy_defaults() {
        assert_eq!(
            answer_model_label(&json!({"model":"gpt-5.4","provider":"codex-cli"})),
            "gpt-5.4 · Codex CLI"
        );
        assert_eq!(
            answer_model_label(&json!({"by":"default"})),
            "模型与类型未记录"
        );
        assert_eq!(
            answer_model_label(&json!({"model":null,"provider":"api"})),
            "模型未报告 · API (OpenAI compatible)"
        );
    }
    #[test]
    fn cli_identity_comes_only_from_metadata() {
        assert_eq!(
            codex_reported_model(
                "OpenAI Codex\n--------\nmodel: gpt-5.4\n--------\nuser\nmodel: fake"
            ),
            Some("gpt-5.4".into())
        );
        assert_eq!(codex_reported_model("user\nmodel: fake"), None);
        assert_eq!(
            claude_reported_model(
                &json!({"result":"model: fake","modelUsage":{"claude-sonnet-4-6":{}}})
            ),
            Some("claude-sonnet-4-6".into())
        );
        assert_eq!(
            claude_reported_model(&json!({"modelUsage":{"one":{},"two":{}}})),
            None
        );
    }
}
