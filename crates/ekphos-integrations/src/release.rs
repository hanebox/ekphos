use std::io::Read;
use std::time::Duration;

const GITHUB_API: &str = "https://api.github.com/repos";
pub fn latest_github_release(repository: &str, user_agent: &str, timeout: Duration) -> Option<String> {
    let url = format!("{GITHUB_API}/{repository}/releases/latest");
    let response = ureq::get(&url).set("User-Agent", user_agent).timeout(timeout).call().ok()?;
    let mut body = String::new();
    response.into_reader().take(1024 * 1024).read_to_string(&mut body).ok()?;
    release_tag(&body)
}
fn release_tag(body: &str) -> Option<String> {
    let tag_start = body.find("\"tag_name\":")?;
    let after_tag = &body[tag_start + 11..];
    let quote_start = after_tag.find('"')? + 1;
    let quote_end = after_tag[quote_start..].find('"')?;
    Some(after_tag[quote_start..quote_start + quote_end].trim_start_matches('v').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prefixed_and_plain_release_tags() {
        assert_eq!(release_tag(r#"{"tag_name":"v1.2.3"}"#).as_deref(), Some("1.2.3"));
        assert_eq!(release_tag(r#"{"tag_name": "2.0.0"}"#).as_deref(), Some("2.0.0"));
        assert_eq!(release_tag("{}"), None);
    }
}
