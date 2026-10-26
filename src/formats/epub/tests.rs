use super::*;

#[test]
fn package_uris_are_normalized_without_escaping_or_using_host_paths() {
    assert_eq!(
        resolve("OEBPS/Text/ch.xhtml", "../Images/a%20b.png").unwrap(),
        ("OEBPS/Images/a b.png".into(), None)
    );
    assert_eq!(
        resolve("OEBPS/Text/ch.xhtml", "#%E4%B8%AD%E6%96%87").unwrap(),
        ("OEBPS/Text/ch.xhtml".into(), Some("中文".into()))
    );
    for uri in [
        "../../../outside",
        "/etc/passwd",
        "file:///etc/passwd",
        "https://host/a",
        "..\\outside",
        "%2Fetc/passwd",
        "%00",
    ] {
        assert!(resolve("OEBPS/Text/ch.xhtml", uri).is_err(), "{uri}");
    }
    assert!(link("OEBPS/Text/ch.xhtml", "javascript:alert(1)").is_none());
    assert!(link("OEBPS/Text/ch.xhtml", "//example.com/image").is_none());
}

#[test]
fn xml_accepts_standard_doctype_but_does_not_load_external_entities_or_deep_trees() {
    assert!(parse(r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd"><html><body>ok</body></html>"#).is_ok());
    assert!(
        parse(
            r#"<!DOCTYPE html [<!ENTITY secret SYSTEM "file:///etc/passwd">]><html>&secret;</html>"#
        )
        .is_err()
    );
    assert!(
        parse(&format!(
            "{}text{}",
            "<div>".repeat(130),
            "</div>".repeat(130)
        ))
        .is_err()
    );
}

#[test]
fn xhtml_named_entities_are_local_and_cdata_stays_literal() {
    let source = normalize_entities(
        r#"<p title="&quot;&copy;">&uuml; &copy; &amp;<![CDATA[&copy;]]><!-- &copy; --></p>"#,
    );
    let doc = parse(&source).unwrap();
    assert_eq!(doc.root_element().attribute("title"), Some("\"©"));
    assert_eq!(text(doc.root_element()), "ü © &&copy;");
    assert!(source.contains("<!-- &copy; -->"));
    assert!(source.contains("<![CDATA[&copy;]]>"));
}

#[test]
fn pre_preserves_publisher_language_and_whitespace_without_token_wrappers() {
    for (attrs, child_attrs, language) in [
        ("data-code-language=\"python\"", "class=\"k\"", "python"),
        ("class=\"lang-python\"", "", "python"),
        ("", "class=\"extra language-python\"", "python"),
    ] {
        let source = format!(
            "<pre {attrs}><code {child_attrs}>def</code> hello():\n    <code>return</code> &quot;中文&quot;\n</pre>"
        );
        let doc = parse(&source).unwrap();
        let html = render(doc.root_element(), "OEBPS/ch.xhtml");
        assert_eq!(
            html,
            format!(
                "<pre><code class=\"language-{language}\">def hello():\n    return &quot;中文&quot;\n</code></pre>"
            )
        );
        assert!(
            markdown(doc.root_element(), false)
                .contains("```python\ndef hello():\n    return \"中文\"\n")
        );
    }
    let doc =
        parse(r#"<pre data-code-language='python&quot; onclick=&quot;bad'>plain output</pre>"#)
            .unwrap();
    assert_eq!(
        render(doc.root_element(), "ch.xhtml"),
        "<pre><code>plain output</code></pre>"
    );
}

#[test]
fn sanitizer_keeps_code_literal_and_drops_executable_attributes() {
    let doc = parse(r#"<p onclick="bad()">literal __init__ <code>&lt;tag&gt; &amp; `tick`</code><script>bad()</script><a href="file:///secret">blocked</a></p>"#).unwrap();
    let html = render(doc.root_element(), "OEBPS/ch.xhtml");
    assert_eq!(
        html,
        "<p><span>literal __init__ <code>&lt;tag&gt; &amp; `tick`</code>blocked</span></p>"
    );
    assert!(markdown(doc.root_element(), false).contains("\\_\\_init\\_\\_"));
}
