use super::{ModelProfile, Provider};

fn profile(provider: Provider, model: &str) -> ModelProfile {
    ModelProfile {
        id: "test".into(),
        name: "Test".into(),
        model: model.into(),
        provider,
        ..Default::default()
    }
}
#[test]
fn thinking_levels_are_provider_specific_and_validated_before_launch() {
    for (provider, model, allowed, rejected) in [
        (
            Provider::Api,
            "gpt-test",
            vec!["", "low", "medium", "high"],
            vec!["xhigh", "max"],
        ),
        (
            Provider::ClaudeCode,
            "claude-opus-5-5",
            vec!["", "low", "medium", "high", "xhigh", "max"],
            vec!["ultra"],
        ),
        (
            Provider::CodexCli,
            "default",
            vec!["", "low", "medium", "high", "xhigh"],
            vec!["max"],
        ),
        (
            Provider::ClaudeCode,
            "claude-haiku-4-5",
            vec![""],
            vec!["low", "high", "max"],
        ),
    ] {
        let mut profile = profile(provider, model);
        for value in allowed {
            profile.reasoning = value.into();
            assert!(profile.validate().is_ok(), "{provider:?}: {value}");
        }
        for value in rejected {
            profile.reasoning = value.into();
            assert!(profile.validate().is_err(), "{provider:?}: {value}");
        }
        profile.reasoning = "high; injected option".into();
        assert!(profile.validate().is_err());
    }
}
#[test]
fn documented_defaults_do_not_invent_api_or_codex_effort() {
    assert_eq!(
        profile(Provider::ClaudeCode, "claude-opus-5-5").reasoning_fallback(),
        "medium"
    );
    assert_eq!(
        profile(Provider::ClaudeCode, "claude-sonnet-5.5").reasoning_fallback(),
        "medium"
    );
    assert_eq!(
        profile(Provider::ClaudeCode, "claude-opus-4-7").reasoning_fallback(),
        "xhigh"
    );
    assert_eq!(
        profile(Provider::ClaudeCode, "sonnet").reasoning_fallback(),
        "high"
    );
    assert_eq!(
        profile(Provider::ClaudeCode, "haiku").reasoning_fallback(),
        ""
    );
    assert_eq!(profile(Provider::Api, "gpt-test").reasoning_fallback(), "");
    assert_eq!(
        profile(Provider::CodexCli, "default").reasoning_fallback(),
        ""
    );
}
