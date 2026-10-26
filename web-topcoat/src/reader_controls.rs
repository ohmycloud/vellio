//! Small server-rendered controls; no client runtime or Tailwind dependency.
use topcoat::{
    Result,
    context::Cx,
    l10n::{Locale, accept_language, fluent::messages, locale, negotiate},
    view::{View, component, element_id, view},
};

messages!("./reader_controls/l10n/{locale}.ftl");

pub fn request_locale(cx: &Cx) -> Locale {
    negotiate(accept_language(cx), t::locales())
        .cloned()
        .unwrap_or(locale!("zh-CN"))
}

#[component]
pub async fn appearance_menu(cx: &Cx) -> Result<impl View> {
    // Only this fully translated control changes language. The surrounding reader
    // and library retain their existing languages; book content is never translated here.
    Ok(view! {
        let panel = element_id(cx);
        let heading = element_id(cx);
        <div class="appearance-menu" lang=(locale(cx).id()) dir=(locale(cx).id().direction())>
            <button type="button" class="tool-button" popovertarget=(&panel) title=(t!(appearance)) aria-label=(t!(appearance))>"Aa"</button>
            <div id=(&panel) popover="auto" class="appearance-popover vellio-popover" data-align="end" aria-labelledby=(&heading)>
                <strong id=(&heading)>(t!(appearance))</strong>
                <label>(t!(font))<select id="reader-font"><option value="noto">(t!(font_noto))</option><option value="system">(t!(font_system))</option><option value="sans">(t!(font_sans))</option></select></label>
                <label>(t!(size))<input id="reader-size" type="range" min="14" max="32" value="21"><output for="reader-size">"21"</output></label>
                <label>(t!(width))<input id="reader-width" type="range" min="25" max="60" value="35"><output for="reader-width">"35"</output></label>
                <label>(t!(leading))<input id="reader-leading" type="range" min="12" max="26" value="19"><output for="reader-leading">"1.9"</output></label>
                <label class="check"><input id="reader-margin" type="checkbox" checked=(true)>(t!(margin_notes))</label>
            </div>
        </div>
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_messages_resolve_for_browser_regions_and_default() {
        assert_eq!(
            t!(appearance).resolve_in(&locale!("en-US")),
            "Reading appearance"
        );
        assert_eq!(t!(appearance).resolve_in(&locale!("zh-CN")), "阅读外观");
        assert_eq!(
            t!(margin_notes).resolve_in(&locale!("en")),
            "Show margin notes"
        );
        let requested = [locale!("fr"), locale!("en-GB")];
        assert_eq!(negotiate(&requested, t::locales()), Some(&locale!("en")));
        assert_eq!(negotiate(&[locale!("fr")], t::locales()), None);
    }
}
